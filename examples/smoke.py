#!/usr/bin/env python3
# Smoke test for the SpiceUsbPortal GIR bindings. Validates that the typelib
# loads, all GObject types resolve, and the async pair
# Devices.new_async/new_finish + enumerate_async/enumerate_finish round-trips.
#
# Usage (after `make install` into ./inst):
#
#   GI_TYPELIB_PATH=$PWD/inst/usr/lib64/girepository-1.0:$PWD/inst/usr/lib/girepository-1.0 \
#   LD_LIBRARY_PATH=$PWD/inst/usr/lib64:$PWD/inst/usr/lib \
#   python3 examples/smoke.py

import sys
import gi

gi.require_version("SpiceUsbPortal", "0.1")

from gi.repository import GLib, SpiceUsbPortal  # noqa: E402

loop = GLib.MainLoop()
exit_code = 0


def on_enumerate(devices, result, _user_data):
    global exit_code
    try:
        store = devices.enumerate_finish(result)
    except GLib.Error as e:
        print(f"enumerate failed: {e.message}", file=sys.stderr)
        exit_code = 1
        loop.quit()
        return

    n = store.get_n_items()
    print(f"Found {n} device(s):")
    for i in range(n):
        desc = store.get_item(i)
        print(
            f"  - id={desc.get_id()}"
            f" vendor={desc.get_vendor()}"
            f" model={desc.get_model()}"
            f" readable={bool(desc.is_readable())}"
            f" writable={bool(desc.is_writable())}"
        )
    loop.quit()


def on_devices_ready(_obj, result, _user_data):
    global exit_code
    try:
        devices = SpiceUsbPortal.Devices.new_finish(result)
    except GLib.Error as e:
        print(f"Devices.new failed: {e.message}", file=sys.stderr)
        exit_code = 1
        loop.quit()
        return
    devices.enumerate_async(None, on_enumerate, None)


print(
    "Resolved GTypes:",
    SpiceUsbPortal.Devices.__gtype__.name,
    SpiceUsbPortal.DeviceDescription.__gtype__.name,
    SpiceUsbPortal.OwnedUsbDevice.__gtype__.name,
    SpiceUsbPortal.Usbredir.__gtype__.name,
)

SpiceUsbPortal.Devices.new_async(None, on_devices_ready, None)
loop.run()
sys.exit(exit_code)
