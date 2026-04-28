//! Bridge between [spice-gtk] (via [`spice-client-glib`]) and the
//! [XDG Desktop USB Portal].
//!
//! For GObject Introspection docs see the [GIR documentation].
//!
//! [spice-gtk]: https://gitlab.freedesktop.org/spice/spice-gtk
//! [`spice-client-glib`]: https://gitlab.gnome.org/theCapypara/spice-gtk-rs
//! [XDG Desktop USB Portal]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Usb.html
//! [GIR documentation]: https://thecapypara.github.io/spice-gtk-usb-portal/
//!
//! # Core types
//!
//! - [`Devices`][devices::Devices] - owns the portal session, lists devices,
//!   and emits signals when devices change.
//! - [`DeviceDescription`][devices::DeviceDescription] - immutable metadata
//!   for one device.
//! - [`OwnedUsbDevice`][devices::OwnedUsbDevice] - RAII handle for a
//!   portal-acquired fd.
//! - [`Usbredir`] - bridge into spice-gtk's `UsbDeviceManager`. Attach an
//!   [`OwnedUsbDevice`][devices::OwnedUsbDevice] to make it visible inside
//!   the SPICE session.
//!
//! # Typical flow
//!
//! Build the SPICE session, create a [`Usbredir`], create a
//! [`Devices`][devices::Devices], then for each device the user picks: ask
//! the portal for an fd via
//! [`acquire_device`][devices::Devices::acquire_device] and hand it to
//! [`Usbredir::attach`].
//!
//! ```ignore
//! use spice_gtk_usb_portal::{Usbredir, WindowIdentifier};
//! use spice_gtk_usb_portal::devices::Devices;
//! use spice_client_glib as spice;
//! # async fn run(session: &spice::Session, parent: &gtk::Window) -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Bridge into spice-gtk's USB manager.
//! let usbredir = Usbredir::new(session)?;
//!
//! // 2. Open a portal session.
//! let devices = Devices::new().await?;
//!
//! // 3. List devices the portal currently sees.
//! for desc in devices.enumerate_devices().await? {
//!     println!("{:?} — {:?} ({})",
//!         desc.vendor(), desc.model(), desc.id());
//! }
//!
//! // 4. Acquire one device (shows the portal's permission dialog) and
//! //    attach it to the SPICE session. `owned` must outlive the attachment;
//! //    drop it to detach and release the fd back to the portal.
//! # let device_id: spice_gtk_usb_portal::DeviceID = todo!();
//! let parent_id = WindowIdentifier::from_native(parent).await;
//! let owned = devices
//!     .acquire_device(parent_id.as_ref(), &device_id, /* writable */ true)
//!     .await?;
//! usbredir.attach(&owned).await?;
//! # Ok(()) }
//! ```
//!
//! Live device updates arrive as GObject signals on
//! [`Devices`][devices::Devices]:
//!
//! ```ignore
//! # use spice_gtk_usb_portal::devices::{Devices, DeviceDescription};
//! # use glib::prelude::*;
//! # fn run(devices: &Devices) {
//! devices.connect_closure(
//!     "device-added",
//!     false,
//!     glib::closure_local!(move |_: Devices, desc: DeviceDescription| {
//!         println!("plugged: {:?}", desc.model());
//!     }),
//! );
//! # }
//! ```
//!
//! See [`examples/client.rs`] in the repository for an end-to-end GTK4 demo
//! that wires this all together with a `rdw4-spice` display.
//!
//! [`examples/client.rs`]: https://github.com/theCapypara/spice-gtk-usb-portal/blob/main/examples/client.rs

pub mod devices;
mod error;
mod usbredir;

#[cfg(feature = "capi")]
mod capi;
pub mod ffi;

pub use ashpd::WindowIdentifier;
pub use ashpd::desktop::usb::DeviceID;

pub use error::Error;
pub use usbredir::{Usbredir, UsbredirError, UsbredirResult};
