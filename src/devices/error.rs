use glib::Quark;
use glib::error::ErrorDomain;

pub type DeviceResult<T> = Result<T, DeviceError>;

#[derive(Debug)]
#[non_exhaustive]
pub enum DeviceError {
    Portal(ashpd::Error),
    Usb(ashpd::desktop::usb::UsbError),
    Init(glib::Error),
}

impl std::error::Error for DeviceError {}

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usb(e) => e.fmt(f),
            Self::Portal(e) => e.fmt(f),
            Self::Init(e) => write!(f, "Initialization error: {}", e),
        }
    }
}

impl From<ashpd::Error> for DeviceError {
    fn from(e: ashpd::Error) -> Self {
        Self::Portal(e)
    }
}

impl From<ashpd::desktop::usb::UsbError> for DeviceError {
    fn from(e: ashpd::desktop::usb::UsbError) -> Self {
        Self::Usb(e)
    }
}

// Error domain for handling errors during async init.
#[derive(Copy, Clone)]
pub(crate) struct SelfErrorDomain;

impl ErrorDomain for SelfErrorDomain {
    fn domain() -> Quark {
        Quark::from_str("spice_gtk_usb_portal_err")
    }

    fn code(self) -> i32 {
        0
    }

    fn from(_code: i32) -> Option<Self>
    where
        Self: Sized,
    {
        Some(Self)
    }
}
