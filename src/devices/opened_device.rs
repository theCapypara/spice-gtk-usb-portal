use ashpd::desktop::usb::{DeviceID, UsbProxy};
use ashpd::zvariant::OwnedFd;
use glib;
use glib::subclass::prelude::*;
use log::{trace, warn};
use spice_client_glib as spice;
use std::cell::RefCell;
use std::mem;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use std::sync::Arc;

#[derive(Debug)]
pub(crate) enum Fd {
    /// Owned fd for the device.
    Owned(OwnedFd),
    /// A raw fd that is now owned by spice-gtk. Use with caution.
    Released(RawFd),
}

impl Default for Fd {
    fn default() -> Self {
        // -1 is an invalid fd; used as a sentinel before the real fd is set
        // and after release_fd has handed ownership to spice-gtk.
        Self::Released(-1)
    }
}

mod imp {
    use super::*;

    #[derive(Default, Debug)]
    pub struct OwnedUsbDevice {
        pub(super) portal_info: RefCell<Option<(Arc<UsbProxy>, DeviceID)>>,
        pub(super) fd: RefCell<Fd>,
        pub(super) spice_device: RefCell<Option<(spice::UsbDeviceManager, spice::UsbDevice)>>,
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

    impl ObjectImpl for OwnedUsbDevice {
        fn dispose(&self) {
            trace!("Dispose OwnedUsbDevice @ {self:?}");
            let portal_info = self.portal_info.borrow_mut().take();
            let spice_info = self.spice_device.borrow_mut().take();
            if portal_info.is_none() && spice_info.is_none() {
                return;
            }
            glib::spawn_future_local(async move {
                if let Some((usb_manager, device)) = spice_info {
                    trace!("dropped usb device - disconnecting from SPICE");
                    if let Err(err) = usb_manager.disconnect_device_future(&device).await {
                        warn!("error during usb device SPICE disconnect: {err}");
                    }
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
}

glib::wrapper! {
    pub struct OwnedUsbDevice(ObjectSubclass<imp::OwnedUsbDevice>);
}

impl OwnedUsbDevice {
    pub(crate) fn new(proxy: &Arc<UsbProxy>, device_id: DeviceID, fd: OwnedFd) -> Self {
        let slf: Self = glib::Object::new();
        let imp = slf.imp();
        imp.portal_info.replace(Some((proxy.clone(), device_id)));
        imp.fd.replace(Fd::Owned(fd));
        trace!("created owned usb device @ {slf:?}");
        slf
    }

    /// Create an OwnedUsbDevice from any owned file descriptor.
    /// The portal release step on drop is skipped for fds acquired this way.
    pub fn from_owned_fd(fd: OwnedFd) -> Self {
        let slf: Self = glib::Object::new();
        slf.imp().fd.replace(Fd::Owned(fd));
        trace!("created owned usb device from owned fd @ {slf:?}");
        slf
    }

    pub fn device_id(&self) -> Option<DeviceID> {
        self.imp()
            .portal_info
            .borrow()
            .as_ref()
            .map(|(_, id)| id.clone())
    }

    pub fn attached(&self) -> bool {
        self.imp().spice_device.borrow().is_some()
    }

    pub async fn detach_from_spice(&self) {
        let spice_info = self.imp().spice_device.borrow_mut().take();
        if let Some((usb_manager, device)) = spice_info {
            trace!("disconnecting device from SPICE");
            if let Err(err) = usb_manager.disconnect_device_future(&device).await {
                warn!("error during usb device SPICE disconnect: {err}");
            }
        }
    }

    /// Release ownership of the fd without closing it. spice-gtk's
    /// libusb backend takes ownership of the fd via
    /// `libusb_wrap_sys_device` and closes it on `libusb_close`;
    /// closing it from here too would double-close the fd.
    pub(crate) fn release_fd(&self) {
        let mut fd_slot = self.imp().fd.borrow_mut();
        let prev = mem::take(&mut *fd_slot);
        *fd_slot = match prev {
            Fd::Owned(fd) => Fd::Released(std::os::fd::OwnedFd::from(fd).into_raw_fd()),
            other => other,
        };
    }

    pub(crate) fn set_spice_device(
        &self,
        manager: spice::UsbDeviceManager,
        device: spice::UsbDevice,
    ) {
        self.imp().spice_device.replace(Some((manager, device)));
    }
}

impl From<OwnedFd> for OwnedUsbDevice {
    fn from(fd: OwnedFd) -> Self {
        Self::from_owned_fd(fd)
    }
}

impl AsRawFd for OwnedUsbDevice {
    fn as_raw_fd(&self) -> RawFd {
        match &*self.imp().fd.borrow() {
            Fd::Owned(fd) => fd.as_raw_fd(),
            Fd::Released(fd) => *fd,
        }
    }
}
