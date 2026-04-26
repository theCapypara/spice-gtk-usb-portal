use ashpd::desktop::usb::{DeviceID, UsbProxy};
use ashpd::zvariant::OwnedFd;
use log::{trace, warn};
use spice_client_glib as spice;
use std::mem;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use std::sync::Arc;

#[derive(Debug)]
enum Fd {
    /// Owned fd for the device.
    Owned(OwnedFd),
    /// A raw fd that is now owned by spice-gtk. Use with caution.
    Released(RawFd),
}

#[derive(Debug)]
pub struct OwnedUsbDevice {
    portal_info: Option<(Arc<UsbProxy>, DeviceID)>,
    fd: Fd,
    pub(crate) spice_device: Option<(spice::UsbDeviceManager, spice::UsbDevice)>,
}

impl OwnedUsbDevice {
    pub(crate) fn new(proxy: &Arc<UsbProxy>, device_id: DeviceID, fd: OwnedFd) -> Self {
        let slf = Self {
            portal_info: Some((proxy.clone(), device_id)),
            fd: Fd::Owned(fd),
            spice_device: None,
        };
        trace!("created owned usb device {slf:?}");
        slf
    }

    pub fn device_id(&self) -> Option<&DeviceID> {
        self.portal_info.as_ref().map(|(_, id)| id)
    }

    pub fn attached(&self) -> bool {
        self.spice_device.is_some()
    }

    pub async fn detach_from_spice(&mut self) {
        if let Some((usb_manager, device)) = self.spice_device.take() {
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
    pub(crate) fn release_fd(&mut self) {
        let fd = mem::replace(&mut self.fd, Fd::Released(0));
        self.fd = match fd {
            Fd::Owned(fd) => Fd::Released(std::os::fd::OwnedFd::from(fd).into_raw_fd()),
            _ => fd,
        }
    }
}

/// Create an OwnedUsbDevice from any owned file descriptor.
impl From<OwnedFd> for OwnedUsbDevice {
    fn from(fd: OwnedFd) -> Self {
        let slf = Self {
            portal_info: None,
            fd: Fd::Owned(fd),
            spice_device: None,
        };
        trace!("created owned usb device {slf:?} from owned fd directly");
        slf
    }
}

impl Drop for OwnedUsbDevice {
    fn drop(&mut self) {
        trace!("dropped usb device {self:?}");
        let portal_info = self.portal_info.take();
        let spice_info = self.spice_device.take();
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

impl AsRawFd for OwnedUsbDevice {
    fn as_raw_fd(&self) -> RawFd {
        match &self.fd {
            Fd::Owned(fd) => fd.as_raw_fd(),
            Fd::Released(fd) => *fd,
        }
    }
}
