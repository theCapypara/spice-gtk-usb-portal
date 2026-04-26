//! C-layout structs for the public GObject types in this crate.
//!
//! These are referenced by `src/capi.rs` and from each subclass via
//! `type Class = ... ; type Instance = ...;` in their `#[glib::object_subclass]`
//! blocks, so that cbindgen emits proper named C structs instead of opaque
//! GObject placeholders.

use glib::gobject_ffi::{GObject, GObjectClass};

#[repr(C)]
pub struct SpiceUsbPortalDevicesClass {
    pub parent_class: GObjectClass,
}

#[repr(C)]
pub struct SpiceUsbPortalDevices {
    parent: GObject,
}

#[repr(C)]
pub struct SpiceUsbPortalDeviceDescriptionClass {
    pub parent_class: GObjectClass,
}

#[repr(C)]
pub struct SpiceUsbPortalDeviceDescription {
    parent: GObject,
}

#[repr(C)]
pub struct SpiceUsbPortalOwnedUsbDeviceClass {
    pub parent_class: GObjectClass,
}

#[repr(C)]
pub struct SpiceUsbPortalOwnedUsbDevice {
    parent: GObject,
}

#[repr(C)]
pub struct SpiceUsbPortalUsbredirClass {
    pub parent_class: GObjectClass,
}

#[repr(C)]
pub struct SpiceUsbPortalUsbredir {
    parent: GObject,
}
