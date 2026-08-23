//! Generic handling of devices acquired from the portal. These devices can be
//! attached to implementations. A SPICE implementation is provided with the `spice` feature
//! and provided as a glib object.

use crate::{UsbredirError, UsbredirResult};
use ashpd::desktop::usb::{DeviceID, UsbProxy};
use ashpd::zvariant::OwnedFd;
use futures_util::future::LocalBoxFuture;
use log::{trace, warn};
use std::fmt::Debug;
use std::mem;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use std::sync::Arc;

#[cfg(feature = "spice")]
pub mod spice;

#[derive(Debug)]
pub(crate) enum Fd {
    /// Owned fd for the device.
    Owned(OwnedFd),
    /// A raw fd that is now owned by the implementation. Use with caution.
    Released(RawFd),
}

impl Default for Fd {
    fn default() -> Self {
        // -1 is an invalid fd; used as a sentinel before the real fd is set
        // and after disown_fd has handed ownership to the backend.
        Self::Released(-1)
    }
}

impl AsRawFd for Fd {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Fd::Owned(fd) => fd.as_raw_fd(),
            Fd::Released(fd) => *fd,
        }
    }
}

#[derive(Debug)]
/// A device fd from the portal. It is owned by the `GenericOwnedUsbDevice` that holds it by default.
/// However, if a backend call transfers ownership, [`Self::disown_fd`] must be called.
pub struct PortalFd(std::sync::Mutex<Fd>);
impl PortalFd {
    fn new(fd: OwnedFd) -> Self {
        Self(std::sync::Mutex::new(Fd::Owned(fd)))
    }

    /// Disown this fd, signaling that it was transferred to some backend that now owns it.
    pub fn disown_fd(&self) {
        let mut fd_slot = self.0.lock().unwrap();
        let prev = mem::take(&mut *fd_slot);
        *fd_slot = match prev {
            Fd::Owned(fd) => Fd::Released(std::os::fd::OwnedFd::from(fd).into_raw_fd()),
            other => other,
        };
    }
}

impl AsRawFd for PortalFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.lock().unwrap().as_raw_fd()
    }
}

#[derive(Debug)]
/// A device acquired from the portal that can be attached to a backend via its
/// [`PortalUsbredirAttacher`].
pub struct GenericOwnedUsbDevice<T: PortalUsbredirAttacher> {
    inner: Arc<Inner<T>>,
}

#[derive(Debug)]
struct Inner<T: PortalUsbredirAttacher> {
    portal_info: Option<(Arc<UsbProxy>, DeviceID)>,
    fd: Arc<PortalFd>,
    /// Only `None` after `drop` has taken it; always `Some` while reachable
    /// through a `GenericOwnedUsbDevice`.
    attacher: Option<futures_util::lock::Mutex<T>>,
}

impl<T: PortalUsbredirAttacher> Inner<T> {
    fn attacher(&self) -> &futures_util::lock::Mutex<T> {
        self.attacher.as_ref().expect("attacher taken in drop")
    }
}

impl<T: PortalUsbredirAttacher> GenericOwnedUsbDevice<T> {
    pub(crate) fn new(
        attacher: T,
        proxy: &Arc<UsbProxy>,
        device_id: DeviceID,
        fd: OwnedFd,
    ) -> Self {
        let slf: Self = Self {
            inner: Arc::new(Inner {
                portal_info: Some((proxy.clone(), device_id)),
                fd: Arc::new(PortalFd::new(fd)),
                attacher: Some(futures_util::lock::Mutex::new(attacher)),
            }),
        };
        trace!("created owned usb device @ {slf:?}");
        slf
    }

    /// Create an GenericOwnedUsbDevice from any owned file descriptor.
    /// The portal release step on drop is skipped for fds acquired this way.
    pub fn from_owned_fd(attacher: T, fd: OwnedFd) -> Self {
        let slf: Self = Self {
            inner: Arc::new(Inner {
                portal_info: None,
                fd: Arc::new(PortalFd::new(fd)),
                attacher: Some(futures_util::lock::Mutex::new(attacher)),
            }),
        };
        trace!("created owned usb device from owned fd @ {slf:?}");
        slf
    }

    pub fn device_id(&self) -> Option<DeviceID> {
        self.inner.portal_info.as_ref().map(|(_, id)| id.clone())
    }

    pub async fn is_attached(&self) -> bool {
        self.inner.attacher().lock().await.is_attached()
    }

    pub fn is_attached_sync(&self) -> bool {
        self.inner
            .attacher()
            .try_lock()
            .map(|a| a.is_attached())
            .unwrap_or(false)
    }

    /// Attach the device. Fails, if it is already attached.
    pub async fn attach(&self, to: &T::AttachBackend) -> UsbredirResult<()> {
        let mut attacher = self.inner.attacher().lock().await;
        if attacher.is_attached() {
            return Err(UsbredirError::AlreadyAttached);
        }

        attacher.attach(self.inner.fd.clone(), to).await
    }

    pub async fn detach(&self) {
        self.inner.attacher().lock().await.detach().await;
    }
}

impl<T: PortalUsbredirAttacher> Drop for Inner<T> {
    fn drop(&mut self) {
        let portal_info = self.portal_info.take();
        // An unattached attacher needs no async teardown; drop it inline.
        let attacher = self
            .attacher
            .take()
            .map(futures_util::lock::Mutex::into_inner)
            .filter(|a| a.is_attached());
        if portal_info.is_none() && attacher.is_none() {
            return;
        }
        glib::spawn_future_local(async move {
            // The device must be detached from the backend before it is handed
            // back to the portal.
            if let Some(mut attacher) = attacher {
                trace!("dropped usb device - detaching from backend");
                attacher.detach().await;
            }
            if let Some((proxy, device_id)) = portal_info {
                trace!("dropped usb device - releasing");
                if let Err(err) = proxy
                    .release_devices(&[&device_id], Default::default())
                    .await
                {
                    warn!("error during usb device release: {err}");
                }
            }
        });
    }
}

impl<T: PortalUsbredirAttacher> AsRawFd for GenericOwnedUsbDevice<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.fd.as_raw_fd()
    }
}

impl<T: PortalUsbredirAttacher> Clone for GenericOwnedUsbDevice<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// A proxy for attaching a device to a backend. When dropped the attacher must detach the device.
pub trait PortalUsbredirAttacher: Debug + 'static {
    /// The type used to actually attach the device.
    type AttachBackend;

    /// Attach the given device. This attacher is after this tied to the device and will
    /// detach it on `detach`.
    /// If any device is already attached by this attacher, this must return
    /// [`UsbredirError::AlreadyAttached`].
    ///
    /// If the backend takes ownership of the fd, the implementation must call
    /// [`PortalFd::disown_fd`] in the same poll as the operation that hands the fd over
    /// (i.e. with no `.await` between them), otherwise a cancelled attach would
    /// close the fd twice.
    fn attach(
        &mut self,
        device: Arc<PortalFd>,
        backend: &Self::AttachBackend,
    ) -> LocalBoxFuture<'_, UsbredirResult<()>>;
    /// Detach a previously attached device.
    fn detach(&mut self) -> LocalBoxFuture<'_, ()>;
    /// Whether this device is currently attached.
    fn is_attached(&self) -> bool;
}
