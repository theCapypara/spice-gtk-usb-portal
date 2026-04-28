#[cfg(feature = "capi")]
use cargo_metadata::*;
#[cfg(feature = "capi")]
use std::path::*;

#[cfg(feature = "capi")]
use cbindgen::Builder;

fn main() {
    #[cfg(feature = "capi")]
    {
        let path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let meta = MetadataCommand::new()
            .manifest_path("./Cargo.toml")
            .current_dir(&path)
            .exec()
            .unwrap();

        let version = &meta.root_package().unwrap().version;
        let name = meta.root_package().unwrap().metadata["capi"]["header"]["name"]
            .as_str()
            .unwrap();
        let macro_prefix = name.replace('-', "_").to_uppercase();
        let out = std::env::var("OUT_DIR").unwrap();
        let out = Path::new(&out);
        let out_include = out.join("capi/include/");
        std::fs::create_dir_all(&out_include).unwrap();

        let mut config = cbindgen::Config::default();
        let warning = config.autogen_warning.unwrap_or_default();
        let version_info = format!(
            r"
#define {0}_MAJOR_VERSION {1}
#define {0}_MINOR_VERSION {2}
#define {0}_PATCH_VERSION {3}

#define {0}_CHECK_VERSION(major,minor,patch)    \
    ({0}_MAJOR_VERSION > (major) || \
     ({0}_MAJOR_VERSION == (major) && {0}_MINOR_VERSION > (minor)) || \
     ({0}_MAJOR_VERSION == (major) && {0}_MINOR_VERSION == (minor) && \
      {0}_PATCH_VERSION >= (patch)))
",
            macro_prefix, version.major, version.minor, version.patch
        );
        config.autogen_warning = Some(warning + &version_info);

        // cbindgen emits #[repr(i32)] enums as `enum NAME { ... };` plus a
        // separate `typedef int32_t NAME;`. g-ir-scanner then sees NAME as an
        // int alias, not an enum, so it cannot tie spice_usb_portal_error_quark()
        // to a GError domain. Skip cbindgen's emit and write a proper
        // `typedef enum { ... } SpiceUsbPortalError;` ourselves with a gtk-doc
        // annotation, so g-ir-scanner registers it as a glib:error-domain.
        config.export.exclude.push("Error".into());
        config.after_includes = Some(
            "\n\
/**\n\
 * SpiceUsbPortalError:\n\
 * @SPICE_USB_PORTAL_ERROR_PORTAL: Error reported by the XDG USB portal.\n\
 * @SPICE_USB_PORTAL_ERROR_USB: USB-level error from the portal.\n\
 * @SPICE_USB_PORTAL_ERROR_NOT_CONNECTED: SPICE session is not connected.\n\
 * @SPICE_USB_PORTAL_ERROR_ATTACH_FAILED: spice-gtk refused to attach the device.\n\
 * @SPICE_USB_PORTAL_ERROR_FAILED: Generic failure not covered by the other codes.\n\
 *\n\
 * Error codes for the %SPICE_USB_PORTAL_ERROR domain.\n\
 */\n\
typedef enum {\n  \
    SPICE_USB_PORTAL_ERROR_PORTAL = 0,\n  \
    SPICE_USB_PORTAL_ERROR_USB = 1,\n  \
    SPICE_USB_PORTAL_ERROR_NOT_CONNECTED = 2,\n  \
    SPICE_USB_PORTAL_ERROR_ATTACH_FAILED = 3,\n  \
    SPICE_USB_PORTAL_ERROR_FAILED = 4,\n\
} SpiceUsbPortalError;\n"
                .to_string(),
        );

        Builder::new()
            .with_crate(&path)
            .with_config(config)
            .with_gobject(true)
            .with_include_version(true)
            .with_include_guard(format!("{}_H", macro_prefix))
            .with_sys_include("gtk/gtk.h")
            .with_sys_include("spice-client.h")
            .generate()
            .unwrap()
            .write_to_file(out_include.join(format!("{name}.h")));
    }
}
