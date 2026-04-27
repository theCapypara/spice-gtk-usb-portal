//! C entry points for the public API.

#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::{CStr, c_char, c_void};
use std::os::fd::AsRawFd;

use ashpd::WindowIdentifier;
use ashpd::desktop::usb::DeviceID;
use gio::ffi::{GAsyncReadyCallback, GAsyncResult, GCancellable};
use gio::prelude::*;
use glib::translate::*;
use glib::types::StaticType;
use spice_client_glib as spice;

use crate::Error;
use crate::devices::{DeviceDescription, Devices, OwnedUsbDevice};
use crate::ffi::{
    SpiceUsbPortalDeviceDescription, SpiceUsbPortalDevices, SpiceUsbPortalOwnedUsbDevice,
    SpiceUsbPortalUsbredir,
};
use crate::usbredir::Usbredir;

// --- error domain ---------------------------------------------------------

/// spice_usb_portal_error_quark:
///
/// Returns: a #GQuark identifying the SpiceUsbPortal error domain.
#[unsafe(no_mangle)]
pub extern "C" fn spice_usb_portal_error_quark() -> glib::ffi::GQuark {
    <Error as glib::error::ErrorDomain>::domain().into_glib()
}

// --- get_type accessors ---------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn spice_usb_portal_devices_get_type() -> glib::ffi::GType {
    Devices::static_type().into_glib()
}

#[unsafe(no_mangle)]
pub extern "C" fn spice_usb_portal_device_description_get_type() -> glib::ffi::GType {
    DeviceDescription::static_type().into_glib()
}

#[unsafe(no_mangle)]
pub extern "C" fn spice_usb_portal_owned_device_get_type() -> glib::ffi::GType {
    OwnedUsbDevice::static_type().into_glib()
}

#[unsafe(no_mangle)]
pub extern "C" fn spice_usb_portal_usbredir_get_type() -> glib::ffi::GType {
    Usbredir::static_type().into_glib()
}

// --- helpers --------------------------------------------------------------

unsafe fn report_error(error_out: *mut *mut glib::ffi::GError, err: glib::Error) {
    if !error_out.is_null() {
        *error_out = err.into_glib_ptr();
    }
}

unsafe fn cstr_to_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        CStr::from_ptr(p).to_str().ok()
    }
}

// --- Devices --------------------------------------------------------------

/// spice_usb_portal_devices_new_async:
/// @cancellable: (nullable): optional #GCancellable
/// @callback: (scope async): a #GAsyncReadyCallback to call when the request is satisfied
/// @user_data: (closure): the data to pass to @callback
///
/// Asynchronously creates a new #SpiceUsbPortalDevices, opening a USB portal
/// session in the process.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_devices_new_async(
    _cancellable: *mut GCancellable,
    callback: GAsyncReadyCallback,
    user_data: *mut c_void,
) {
    let callback = callback.expect("callback must not be NULL");
    let user_data = user_data as usize;

    let closure = move |task: gio::LocalTask<Devices>, _: Option<&glib::Object>| {
        let result: *mut GAsyncResult = task.upcast_ref::<gio::AsyncResult>().to_glib_none().0;
        callback(std::ptr::null_mut(), result, user_data as *mut c_void)
    };

    let task: gio::LocalTask<Devices> =
        gio::LocalTask::new(None::<&glib::Object>, gio::Cancellable::NONE, closure);

    glib::MainContext::default().spawn_local(async move {
        let res = Devices::new().await.map_err(glib::Error::from);
        task.return_result(res);
    });
}

/// spice_usb_portal_devices_new_finish:
/// @res: a #GAsyncResult
/// @error: (out callee-allocates) (optional): return location for a #GError
///
/// Returns: (transfer full) (nullable): the new #SpiceUsbPortalDevices, or %NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_devices_new_finish(
    res: *mut GAsyncResult,
    error: *mut *mut glib::ffi::GError,
) -> *mut SpiceUsbPortalDevices {
    let task: gio::LocalTask<Devices> = from_glib_none(res as *mut gio::ffi::GTask);
    match task.propagate() {
        Ok(devices) => {
            let ptr: *mut SpiceUsbPortalDevices = devices.to_glib_full();
            ptr
        }
        Err(e) => {
            report_error(error, e);
            std::ptr::null_mut()
        }
    }
}

/// spice_usb_portal_devices_enumerate_async:
/// @self: a #SpiceUsbPortalDevices
/// @cancellable: (nullable): optional #GCancellable
/// @callback: (scope async): a #GAsyncReadyCallback
/// @user_data: (closure): closure data
///
/// Asynchronously enumerates the USB devices currently visible to the portal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_devices_enumerate_async(
    this: *mut SpiceUsbPortalDevices,
    _cancellable: *mut GCancellable,
    callback: GAsyncReadyCallback,
    user_data: *mut c_void,
) {
    let callback = callback.expect("callback must not be NULL");
    let user_data = user_data as usize;
    let this_ptr = this;
    let this: Devices = from_glib_none(this);

    let closure = move |task: gio::LocalTask<gio::ListStore>, _: Option<&Devices>| {
        let result: *mut GAsyncResult = task.upcast_ref::<gio::AsyncResult>().to_glib_none().0;
        callback(this_ptr as *mut _, result, user_data as *mut c_void)
    };

    let task: gio::LocalTask<gio::ListStore> =
        gio::LocalTask::new(Some(&this), gio::Cancellable::NONE, closure);

    glib::MainContext::default().spawn_local(async move {
        let res = match this.enumerate_devices().await {
            Ok(list) => {
                let store = gio::ListStore::new::<DeviceDescription>();
                for d in list {
                    store.append(&d);
                }
                Ok(store)
            }
            Err(e) => Err(glib::Error::from(e)),
        };
        task.return_result(res);
    });
}

/// spice_usb_portal_devices_enumerate_finish:
/// @self: a #SpiceUsbPortalDevices
/// @res: a #GAsyncResult
/// @error: (out callee-allocates) (optional): return location for a #GError
///
/// Returns: (transfer full) (nullable) (element-type SpiceUsbPortalDeviceDescription):
///   a #GListStore of #SpiceUsbPortalDeviceDescription, or %NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_devices_enumerate_finish(
    _this: *mut SpiceUsbPortalDevices,
    res: *mut GAsyncResult,
    error: *mut *mut glib::ffi::GError,
) -> *mut gio::ffi::GListStore {
    let task: gio::LocalTask<gio::ListStore> = from_glib_none(res as *mut gio::ffi::GTask);
    match task.propagate() {
        Ok(store) => store.to_glib_full(),
        Err(e) => {
            report_error(error, e);
            std::ptr::null_mut()
        }
    }
}

/// spice_usb_portal_devices_acquire_device_async:
/// @self: a #SpiceUsbPortalDevices
/// @parent_window: (nullable): optional parent #GtkWindow for the portal dialog
/// @device_id: the device id, as obtained from spice_usb_portal_device_description_get_id()
/// @writable: whether to request write access
/// @cancellable: (nullable): optional #GCancellable
/// @callback: (scope async): a #GAsyncReadyCallback
/// @user_data: (closure): closure data
///
/// Asks the portal to open the device and returns an owned handle on completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_devices_acquire_device_async(
    this: *mut SpiceUsbPortalDevices,
    parent_window: *mut gtk::ffi::GtkWindow,
    device_id: *const c_char,
    writable: glib::ffi::gboolean,
    _cancellable: *mut GCancellable,
    callback: GAsyncReadyCallback,
    user_data: *mut c_void,
) {
    let callback = callback.expect("callback must not be NULL");
    let user_data = user_data as usize;
    let this_ptr = this;
    let this: Devices = from_glib_none(this);
    let writable = writable != glib::ffi::GFALSE;
    let device_id_str = match cstr_to_str(device_id) {
        Some(s) => s.to_owned(),
        None => {
            // Fail synchronously by scheduling an error result.
            let closure = move |task: gio::LocalTask<OwnedUsbDevice>, _: Option<&Devices>| {
                let result: *mut GAsyncResult =
                    task.upcast_ref::<gio::AsyncResult>().to_glib_none().0;
                callback(this_ptr as *mut _, result, user_data as *mut c_void)
            };
            let task: gio::LocalTask<OwnedUsbDevice> =
                gio::LocalTask::new(Some(&this), gio::Cancellable::NONE, closure);
            task.return_result(Err(glib::Error::new(
                Error::Failed,
                "device_id must be a non-NULL UTF-8 string",
            )));
            return;
        }
    };

    let parent: Option<gtk::Window> = if parent_window.is_null() {
        None
    } else {
        Some(from_glib_none(parent_window))
    };

    let closure = move |task: gio::LocalTask<OwnedUsbDevice>, _: Option<&Devices>| {
        let result: *mut GAsyncResult = task.upcast_ref::<gio::AsyncResult>().to_glib_none().0;
        callback(this_ptr as *mut _, result, user_data as *mut c_void)
    };

    let task: gio::LocalTask<OwnedUsbDevice> =
        gio::LocalTask::new(Some(&this), gio::Cancellable::NONE, closure);

    glib::MainContext::default().spawn_local(async move {
        let parent_id = match parent {
            Some(w) => WindowIdentifier::from_native(&w).await,
            None => None,
        };
        let id = DeviceID::from(device_id_str);
        let res = this
            .acquire_device(parent_id.as_ref(), &id, writable)
            .await
            .map_err(glib::Error::from);
        task.return_result(res);
    });
}

/// spice_usb_portal_devices_acquire_device_finish:
/// @self: a #SpiceUsbPortalDevices
/// @res: a #GAsyncResult
/// @error: (out callee-allocates) (optional): return location for a #GError
///
/// Returns: (transfer full) (nullable): a #SpiceUsbPortalOwnedUsbDevice, or %NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_devices_acquire_device_finish(
    _this: *mut SpiceUsbPortalDevices,
    res: *mut GAsyncResult,
    error: *mut *mut glib::ffi::GError,
) -> *mut SpiceUsbPortalOwnedUsbDevice {
    let task: gio::LocalTask<OwnedUsbDevice> = from_glib_none(res as *mut gio::ffi::GTask);
    match task.propagate() {
        Ok(dev) => {
            let ptr: *mut SpiceUsbPortalOwnedUsbDevice = dev.to_glib_full();
            ptr
        }
        Err(e) => {
            report_error(error, e);
            std::ptr::null_mut()
        }
    }
}

// --- DeviceDescription ----------------------------------------------------

/// spice_usb_portal_device_description_get_id:
/// @self: a #SpiceUsbPortalDeviceDescription
///
/// Returns: (transfer full): the portal device id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_device_description_get_id(
    this: *mut SpiceUsbPortalDeviceDescription,
) -> *mut c_char {
    let this: DeviceDescription = from_glib_none(this);
    let id: &DeviceID = this.id();
    let s: &str = id.as_ref();
    s.to_owned().to_glib_full()
}

/// spice_usb_portal_device_description_get_model:
/// @self: a #SpiceUsbPortalDeviceDescription
///
/// Returns: (transfer full) (nullable): the device model string, or %NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_device_description_get_model(
    this: *mut SpiceUsbPortalDeviceDescription,
) -> *mut c_char {
    let this: DeviceDescription = from_glib_none(this);
    this.model().to_glib_full()
}

/// spice_usb_portal_device_description_get_vendor:
/// @self: a #SpiceUsbPortalDeviceDescription
///
/// Returns: (transfer full) (nullable): the device vendor string, or %NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_device_description_get_vendor(
    this: *mut SpiceUsbPortalDeviceDescription,
) -> *mut c_char {
    let this: DeviceDescription = from_glib_none(this);
    this.vendor().to_glib_full()
}

/// spice_usb_portal_device_description_get_parent_id:
/// @self: a #SpiceUsbPortalDeviceDescription
///
/// Returns: (transfer full) (nullable): the parent device id, or %NULL if this is a root device.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_device_description_get_parent_id(
    this: *mut SpiceUsbPortalDeviceDescription,
) -> *mut c_char {
    let this: DeviceDescription = from_glib_none(this);
    match this.parent_id() {
        Some(id) => {
            let s: &str = id.as_ref();
            s.to_owned().to_glib_full()
        }
        None => std::ptr::null_mut(),
    }
}

/// spice_usb_portal_device_description_is_readable:
/// @self: a #SpiceUsbPortalDeviceDescription
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_device_description_is_readable(
    this: *mut SpiceUsbPortalDeviceDescription,
) -> glib::ffi::gboolean {
    let this: DeviceDescription = from_glib_none(this);
    this.readable().into_glib()
}

/// spice_usb_portal_device_description_is_writable:
/// @self: a #SpiceUsbPortalDeviceDescription
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_device_description_is_writable(
    this: *mut SpiceUsbPortalDeviceDescription,
) -> glib::ffi::gboolean {
    let this: DeviceDescription = from_glib_none(this);
    this.writable().into_glib()
}

/// spice_usb_portal_device_description_is_likely_usb_hub:
/// @self: a #SpiceUsbPortalDeviceDescription
///
/// A heuristic match against the model string for hubs and billboard devices,
/// useful for filtering them out of user-facing device lists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_device_description_is_likely_usb_hub(
    this: *mut SpiceUsbPortalDeviceDescription,
) -> glib::ffi::gboolean {
    let this: DeviceDescription = from_glib_none(this);
    this.is_likely_usb_hub().into_glib()
}

// --- OwnedUsbDevice -------------------------------------------------------

/// spice_usb_portal_owned_device_get_device_id:
/// @self: a #SpiceUsbPortalOwnedUsbDevice
///
/// Returns: (transfer full) (nullable): the portal device id, or %NULL when the
///   handle was constructed from a raw file descriptor (no portal session).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_owned_device_get_device_id(
    this: *mut SpiceUsbPortalOwnedUsbDevice,
) -> *mut c_char {
    let this: OwnedUsbDevice = from_glib_none(this);
    match this.device_id() {
        Some(id) => {
            let s: &str = id.as_ref();
            s.to_owned().to_glib_full()
        }
        None => std::ptr::null_mut(),
    }
}

/// spice_usb_portal_owned_device_is_attached:
/// @self: a #SpiceUsbPortalOwnedUsbDevice
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_owned_device_is_attached(
    this: *mut SpiceUsbPortalOwnedUsbDevice,
) -> glib::ffi::gboolean {
    let this: OwnedUsbDevice = from_glib_none(this);
    this.attached().into_glib()
}

/// spice_usb_portal_owned_device_get_fd:
/// @self: a #SpiceUsbPortalOwnedUsbDevice
///
/// Returns: the underlying file descriptor (or -1 once spice-gtk has taken
///   ownership). Do not close it manually.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_owned_device_get_fd(
    this: *mut SpiceUsbPortalOwnedUsbDevice,
) -> i32 {
    let this: OwnedUsbDevice = from_glib_none(this);
    this.as_raw_fd()
}

/// spice_usb_portal_owned_device_detach_from_spice_async:
/// @self: a #SpiceUsbPortalOwnedUsbDevice
/// @callback: (scope async): a #GAsyncReadyCallback
/// @user_data: (closure): closure data
///
/// Disconnects this device from spice-gtk's USB redirection. Idempotent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_owned_device_detach_from_spice_async(
    this: *mut SpiceUsbPortalOwnedUsbDevice,
    callback: GAsyncReadyCallback,
    user_data: *mut c_void,
) {
    let callback = callback.expect("callback must not be NULL");
    let user_data = user_data as usize;
    let this_ptr = this;
    let this: OwnedUsbDevice = from_glib_none(this);

    let closure = move |task: gio::LocalTask<bool>, _: Option<&OwnedUsbDevice>| {
        let result: *mut GAsyncResult = task.upcast_ref::<gio::AsyncResult>().to_glib_none().0;
        callback(this_ptr as *mut _, result, user_data as *mut c_void)
    };

    let task: gio::LocalTask<bool> =
        gio::LocalTask::new(Some(&this), gio::Cancellable::NONE, closure);

    glib::MainContext::default().spawn_local(async move {
        this.detach_from_spice().await;
        task.return_result(Ok(true));
    });
}

/// spice_usb_portal_owned_device_detach_from_spice_finish:
/// @self: a #SpiceUsbPortalOwnedUsbDevice
/// @res: a #GAsyncResult
/// @error: (out callee-allocates) (optional): return location for a #GError
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_owned_device_detach_from_spice_finish(
    _this: *mut SpiceUsbPortalOwnedUsbDevice,
    res: *mut GAsyncResult,
    error: *mut *mut glib::ffi::GError,
) -> glib::ffi::gboolean {
    let task: gio::LocalTask<bool> = from_glib_none(res as *mut gio::ffi::GTask);
    match task.propagate() {
        Ok(_) => glib::ffi::GTRUE,
        Err(e) => {
            report_error(error, e);
            glib::ffi::GFALSE
        }
    }
}

// --- Usbredir -------------------------------------------------------------

/// spice_usb_portal_usbredir_new:
/// @session: a #SpiceSession
/// @error: (out callee-allocates) (optional): return location for a #GError
///
/// Creates a new redirector bound to the given SPICE session.
///
/// Returns: (transfer full) (nullable): the new #SpiceUsbPortalUsbredir, or %NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_usbredir_new(
    session: *mut spice::ffi::SpiceSession,
    error: *mut *mut glib::ffi::GError,
) -> *mut SpiceUsbPortalUsbredir {
    if session.is_null() {
        report_error(
            error,
            glib::Error::new(Error::Failed, "session must not be NULL"),
        );
        return std::ptr::null_mut();
    }
    let session: spice::Session = from_glib_none(session);
    match Usbredir::new(&session) {
        Ok(u) => {
            let ptr: *mut SpiceUsbPortalUsbredir = u.to_glib_full();
            ptr
        }
        Err(e) => {
            report_error(error, glib::Error::from(e));
            std::ptr::null_mut()
        }
    }
}

/// spice_usb_portal_usbredir_attach_async:
/// @self: a #SpiceUsbPortalUsbredir
/// @device: a #SpiceUsbPortalOwnedUsbDevice acquired via the portal
/// @callback: (scope async): a #GAsyncReadyCallback
/// @user_data: (closure): closure data
///
/// Attach a USB device to the SPICE session.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_usbredir_attach_async(
    this: *mut SpiceUsbPortalUsbredir,
    device: *mut SpiceUsbPortalOwnedUsbDevice,
    callback: GAsyncReadyCallback,
    user_data: *mut c_void,
) {
    let callback = callback.expect("callback must not be NULL");
    let user_data = user_data as usize;
    let this_ptr = this;
    let this: Usbredir = from_glib_none(this);
    let device: OwnedUsbDevice = from_glib_none(device);

    let closure = move |task: gio::LocalTask<bool>, _: Option<&Usbredir>| {
        let result: *mut GAsyncResult = task.upcast_ref::<gio::AsyncResult>().to_glib_none().0;
        callback(this_ptr as *mut _, result, user_data as *mut c_void)
    };

    let task: gio::LocalTask<bool> =
        gio::LocalTask::new(Some(&this), gio::Cancellable::NONE, closure);

    glib::MainContext::default().spawn_local(async move {
        let res = this
            .attach(&device)
            .await
            .map(|_| true)
            .map_err(glib::Error::from);
        task.return_result(res);
    });
}

/// spice_usb_portal_usbredir_attach_finish:
/// @self: a #SpiceUsbPortalUsbredir
/// @res: a #GAsyncResult
/// @error: (out callee-allocates) (optional): return location for a #GError
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spice_usb_portal_usbredir_attach_finish(
    _this: *mut SpiceUsbPortalUsbredir,
    res: *mut GAsyncResult,
    error: *mut *mut glib::ffi::GError,
) -> glib::ffi::gboolean {
    let task: gio::LocalTask<bool> = from_glib_none(res as *mut gio::ffi::GTask);
    match task.propagate() {
        Ok(_) => glib::ffi::GTRUE,
        Err(e) => {
            report_error(error, e);
            glib::ffi::GFALSE
        }
    }
}
