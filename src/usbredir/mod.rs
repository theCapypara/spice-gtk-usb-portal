pub(crate) mod error;
#[cfg(feature = "spice")]
mod spice_impl;

#[cfg(feature = "spice")]
pub use spice_impl::*;
