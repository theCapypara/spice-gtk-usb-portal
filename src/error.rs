use crate::UsbredirError;
use crate::devices::DeviceError;

#[derive(Debug, Copy, Clone, PartialEq, Eq, glib::ErrorDomain)]
#[error_domain(name = "SpiceUsbPortalError")]
#[repr(i32)]
pub enum Error {
    Portal = 0,
    Usb = 1,
    NotConnected = 2,
    AttachFailed = 3,
    Failed = 4,
}

impl From<DeviceError> for glib::Error {
    fn from(e: DeviceError) -> Self {
        match &e {
            DeviceError::Portal(_) => glib::Error::new(Error::Portal, &e.to_string()),
            DeviceError::Usb(_) => glib::Error::new(Error::Usb, &e.to_string()),
            DeviceError::Init(g) => g.clone(),
        }
    }
}

impl From<UsbredirError> for glib::Error {
    fn from(e: UsbredirError) -> Self {
        match &e {
            UsbredirError::Glib(g) => g.clone(),
            UsbredirError::NotConnected => glib::Error::new(Error::NotConnected, &e.to_string()),
            UsbredirError::DeviceAttachFailed => {
                glib::Error::new(Error::AttachFailed, &e.to_string())
            }
        }
    }
}
