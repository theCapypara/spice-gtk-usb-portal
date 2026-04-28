Title: Overview
Slug: overview

# Overview

`SpiceUsbPortal` bridges [spice-gtk](https://gitlab.freedesktop.org/spice/spice-gtk)
and the [XDG Desktop USB Portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Usb.html).

The same API is available from Rust via the
[`spice-gtk-usb-portal` crate](https://docs.rs/spice-gtk-usb-portal).

## Core types

- [class@Devices] - owns the portal session, lists devices, and emits
  signals when devices change.
- [class@DeviceDescription] - immutable metadata for one device.
- [class@OwnedUsbDevice] - RAII handle for a portal-acquired fd.
- [class@Usbredir] - bridge into spice-gtk's `SpiceUsbDeviceManager`. Attach
  an [class@OwnedUsbDevice] to make it visible inside the SPICE session.

## Typical flow

Build the SPICE session, create a [class@Usbredir], create a [class@Devices],
then for each device the user picks: ask the portal for an fd via
[method@Devices.acquire_device_async] and hand it to
[method@Usbredir.attach_async].

```c
#include <gtk/gtk.h>
#include <spice-client-glib-2.0/spice-client.h>
#include <spice-gtk-usb-portal/spice-usb-portal.h>

/* Per-attach context: keeps the device handle alive until detach.
 * Dropping the last ref runs the portal release in the background. */
typedef struct {
    SpiceUsbPortalUsbredir       *usbredir;
    SpiceUsbPortalOwnedUsbDevice *owned;
} AttachCtx;

static void
on_attached (GObject *src, GAsyncResult *res, gpointer user_data)
{
    AttachCtx   *ctx = user_data;
    GError      *err = NULL;

    if (!spice_usb_portal_usbredir_attach_finish (ctx->usbredir, res, &err)) {
        g_warning ("attach failed: %s", err->message);
        g_clear_error (&err);
        g_clear_object (&ctx->owned);   /* releases fd back to portal */
        g_free (ctx);
        return;
    }
    /* Keep `ctx->owned` alive - drop it from your app state on detach. */
}

static void
on_acquired (GObject *src, GAsyncResult *res, gpointer user_data)
{
    SpiceUsbPortalDevices       *devices = SPICE_USB_PORTAL_DEVICES (src);
    SpiceUsbPortalUsbredir      *usbredir = user_data;
    GError                      *err = NULL;
    SpiceUsbPortalOwnedUsbDevice *owned;

    owned = spice_usb_portal_devices_acquire_device_finish (devices, res, &err);
    if (!owned) {
        g_warning ("acquire failed: %s", err->message);
        g_clear_error (&err);
        return;
    }

    AttachCtx *ctx = g_new0 (AttachCtx, 1);
    ctx->usbredir = usbredir;
    ctx->owned    = owned;
    spice_usb_portal_usbredir_attach_async (usbredir, owned, on_attached, ctx);
}

static void
on_devices_ready (GObject *src, GAsyncResult *res, gpointer user_data)
{
    SpiceSession           *session  = user_data;
    GError                 *err      = NULL;
    SpiceUsbPortalDevices  *devices;
    SpiceUsbPortalUsbredir *usbredir;

    devices = spice_usb_portal_devices_new_finish (res, &err);
    if (!devices) {
        g_warning ("portal init failed: %s", err->message);
        g_clear_error (&err);
        return;
    }

    usbredir = spice_usb_portal_usbredir_new (session, &err);
    if (!usbredir) {
        g_warning ("usbredir init failed: %s", err->message);
        g_clear_error (&err);
        g_object_unref (devices);
        return;
    }

    /* Pick a device id (e.g. from an "device-added" signal handler or
     * spice_usb_portal_devices_enumerate_async()), then acquire it. */
    const char *device_id = "foo-bar";
    spice_usb_portal_devices_acquire_device_async (
        devices,
        /* parent_window */ NULL,
        device_id,
        /* writable */ TRUE,
        /* cancellable */ NULL,
        on_acquired,
        usbredir);
}

static void
start (SpiceSession *session)
{
    /* Create the portal-side state. */
    spice_usb_portal_devices_new_async (NULL, on_devices_ready, session);
}
```

Live device updates arrive as GObject signals on [class@Devices]:

```c
static void
on_device_added (SpiceUsbPortalDevices          *devices,
                 SpiceUsbPortalDeviceDescription *desc,
                 gpointer                         user_data)
{
    g_autofree char *vendor = spice_usb_portal_device_description_get_vendor (desc);
    g_autofree char *model  = spice_usb_portal_device_description_get_model  (desc);
    g_print ("plugged: %s - %s\n", vendor, model);
}

g_signal_connect (devices, "device-added",   G_CALLBACK (on_device_added), NULL);
g_signal_connect (devices, "device-changed", G_CALLBACK (on_device_added), NULL);
```

## Errors

All fallible entry points report `GError` in the `SPICE_USB_PORTAL_ERROR`
domain (see [error@Error]).

## See also

- [Source repository](https://github.com/theCapypara/spice-gtk-usb-portal)
- [Rust API documentation on docs.rs](https://docs.rs/spice-gtk-usb-portal)
- [`examples/client.rs`](https://github.com/theCapypara/spice-gtk-usb-portal/blob/main/examples/client.rs) - end-to-end
  GTK4 demo (Rust)
- [`examples/smoke.py`](https://github.com/theCapypara/spice-gtk-usb-portal/blob/main/examples/smoke.py) - minimal
  PyGObject smoke test
