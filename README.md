# spice-gtk-usb-portal

A Rust bridge between [spice-gtk](https://gitlab.freedesktop.org/spice/spice-gtk)
(via [`spice-client-glib`](https://gitlab.gnome.org/malureau/spice-gtk-rs))
and the [XDG Desktop USB Portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Usb.html).

It enables applications to redirect USB devices over a SPICE connection using
that portal. 

Unlike `spice-gtk`'s builtin device management, which relies on
`libusb` opening devices itself, requiring direct access to `/dev/bus/usb`
and a helper binary with elevated permissions, the portal-based approach
requires only portal access and is suitable for sandboxed applications.
This approach only has the drawback of requiring users to manage write 
permissions to their devices themselves.

## Release Status

This crate is **not yet available on crates.io** and currently has two hard
prerequisites that will be lifted over time:

1. **spice-gtk should be built from a not-yet-merged branch.**
   The stable `v0.42` has a bug that otherwise causes crashes whenever
   USB devices are disconnected.
   This bug is currently being addressed in [merge request !144](https://gitlab.freedesktop.org/spice/spice-gtk/-/merge_requests/144).

2. **`spice-client-glib` is pinned to a git revision.** The
   [`spice-gtk-rs`](https://gitlab.gnome.org/theCapypara/spice-gtk-rs) bindings are currently pinned to a version
   that adds support for `spice_usb_backend_allocate_device_for_file_descriptor`.

Once MR !144 is merged and new releases of spice-gtk and spice-gtk-rs are
released, these prerequisites will be lifed.

## Usage

See [`examples/client.rs`](examples/client.rs) for an example.

Currently, this library can only be used as a Rust-library, C bindings
don't exist yet.
