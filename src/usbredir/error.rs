pub type UsbredirResult<T> = Result<T, UsbredirError>;

#[derive(Debug)]
#[non_exhaustive]
pub enum UsbredirError {
    Glib(glib::Error),
    NotConnected,
    DeviceAttachFailed,
}

impl std::error::Error for UsbredirError {}

impl std::fmt::Display for UsbredirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Glib(e) => e.fmt(f),
            Self::NotConnected => f.write_str("the SPICE session is not connected"),
            Self::DeviceAttachFailed => {
                f.write_str("the device could not be attached to the SPICE session")
            }
        }
    }
}

impl From<glib::Error> for UsbredirError {
    fn from(e: glib::Error) -> Self {
        Self::Glib(e)
    }
}
