//! OwnedUsbDevice: A device acquired from the portal, that can be attached to spice-gtk.
//! Exposed as a glib object.
use crate::devices::PortalUsbredirAttacher;
use crate::devices::opened_device::{GenericOwnedUsbDevice, PortalFd};
use crate::{UsbredirError, UsbredirResult};
use ashpd::desktop::usb::DeviceID;
use ashpd::zvariant::OwnedFd;
use futures_util::future::LocalBoxFuture;
use glib;
use glib::prelude::*;
use glib::subclass::prelude::*;
use log::{trace, warn};
use spice_client_glib as spice;
use std::cell::OnceCell;
use std::cell::RefCell;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

#[derive(Default, Debug)]
pub(crate) struct SpicePortalUsbredirAttacher {
    spice_device: RefCell<Option<(spice::UsbDeviceManager, spice::UsbDevice)>>,
}
impl PortalUsbredirAttacher for SpicePortalUsbredirAttacher {
    type AttachBackend = spice::UsbDeviceManager;

    fn attach(
        &mut self,
        device: Arc<PortalFd>,
        usb_manager: &Self::AttachBackend,
    ) -> LocalBoxFuture<'_, UsbredirResult<()>> {
        let usb_manager = usb_manager.clone();
        Box::pin(async move {
            trace!("attaching device");
            if self.spice_device.borrow().is_some() {
                return Err(UsbredirError::AlreadyAttached);
            }

            let Some(spice_device) =
                usb_manager.allocate_device_for_file_descriptor(device.as_raw_fd())?
            else {
                return Err(UsbredirError::DeviceAttachFailed);
            };
            device.disown_fd();

            trace!("spice device created");

            usb_manager.can_redirect_device(&spice_device)?;
            usb_manager.connect_device_future(&spice_device).await?;

            trace!("device connected");

            self.spice_device
                .replace(Some((usb_manager.clone(), spice_device)));

            usb_manager.notify("free-channels");
            Ok(())
        })
    }

    fn detach(&mut self) -> LocalBoxFuture<'_, ()> {
        Box::pin(disconnect(self.spice_device.take()))
    }

    fn is_attached(&self) -> bool {
        self.spice_device.borrow().is_some()
    }
}

/// Disconnect a previously connected device from the SPICE manager.
async fn disconnect(spice_info: Option<(spice::UsbDeviceManager, spice::UsbDevice)>) {
    if let Some((usb_manager, device)) = spice_info {
        trace!("disconnecting device from SPICE");
        if let Err(err) = usb_manager.disconnect_device_future(&device).await {
            warn!("error during usb device SPICE disconnect: {err}");
        }

        usb_manager.notify("free-channels");
    }
}

impl Drop for SpicePortalUsbredirAttacher {
    fn drop(&mut self) {
        // Backstop for attachers dropped outside GenericOwnedUsbDevice's
        // teardown (which detaches before dropping); usually a no-op.
        let spice_info = self.spice_device.borrow_mut().take();
        if spice_info.is_some() {
            glib::spawn_future_local(disconnect(spice_info));
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default, Debug)]
    pub struct OwnedUsbDevice {
        pub(super) inner: OnceCell<GenericOwnedUsbDevice<SpicePortalUsbredirAttacher>>,
    }

    unsafe impl ClassStruct for crate::ffi::SpiceUsbPortalOwnedUsbDeviceClass {
        type Type = OwnedUsbDevice;
    }

    unsafe impl InstanceStruct for crate::ffi::SpiceUsbPortalOwnedUsbDevice {
        type Type = OwnedUsbDevice;
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OwnedUsbDevice {
        const NAME: &'static str = "SpiceUsbPortalOwnedUsbDevice";
        type Type = super::OwnedUsbDevice;
        type ParentType = glib::Object;
        type Class = crate::ffi::SpiceUsbPortalOwnedUsbDeviceClass;
        type Instance = crate::ffi::SpiceUsbPortalOwnedUsbDevice;
    }

    impl ObjectImpl for OwnedUsbDevice {}
}

glib::wrapper! {
    pub struct OwnedUsbDevice(ObjectSubclass<imp::OwnedUsbDevice>);
}

impl OwnedUsbDevice {
    pub(crate) fn new(inner: GenericOwnedUsbDevice<SpicePortalUsbredirAttacher>) -> Self {
        let slf: Self = glib::Object::new();
        slf.imp().inner.set(inner).unwrap();
        slf
    }

    /// Create an OwnedUsbDevice from any owned file descriptor.
    /// The portal release step on drop is skipped for fds acquired this way.
    pub fn from_owned_fd(fd: OwnedFd) -> Self {
        let slf: Self = glib::Object::new();
        let inner =
            GenericOwnedUsbDevice::from_owned_fd(SpicePortalUsbredirAttacher::default(), fd);
        slf.imp().inner.set(inner).unwrap();
        slf
    }

    pub fn device_id(&self) -> Option<DeviceID> {
        self.imp()
            .inner
            .get()
            .map(GenericOwnedUsbDevice::device_id)
            .unwrap_or_default()
    }

    pub async fn is_attached(&self) -> bool {
        if let Some(dev) = self.imp().inner.get() {
            dev.is_attached().await
        } else {
            false
        }
    }

    pub fn is_attached_sync(&self) -> bool {
        self.imp()
            .inner
            .get()
            .map(GenericOwnedUsbDevice::is_attached_sync)
            .unwrap_or_default()
    }

    pub async fn attach_to_spice(
        &self,
        usb_manager: &spice::UsbDeviceManager,
    ) -> UsbredirResult<()> {
        // `inner` is only unset for instances created via `g_object_new` from
        // the C/GIR side; fail instead of panicking across the FFI boundary.
        let Some(inner) = self.imp().inner.get() else {
            warn!("OwnedUsbDevice used without initialization - construct it via acquire_device");
            return Err(UsbredirError::DeviceAttachFailed);
        };
        inner.attach(usb_manager).await
    }

    pub async fn detach_from_spice(&self) {
        if let Some(inner) = self.imp().inner.get() {
            inner.detach().await
        }
    }
}

impl From<OwnedFd> for OwnedUsbDevice {
    fn from(fd: OwnedFd) -> Self {
        Self::from_owned_fd(fd)
    }
}

impl AsRawFd for OwnedUsbDevice {
    fn as_raw_fd(&self) -> RawFd {
        // -1 for instances created via `g_object_new` from the C/GIR side
        // (matching the documented C behavior) instead of panicking across
        // the FFI boundary.
        self.imp().inner.get().map(AsRawFd::as_raw_fd).unwrap_or(-1)
    }
}
