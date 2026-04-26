use gtk::gio;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};
use log::{error, info, warn};
use spice_gtk_usb_portal::devices::{DeviceDescription, DeviceError, Devices, OwnedUsbDevice};
use spice_gtk_usb_portal::{DeviceID, Usbredir, WindowIdentifier};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use stderrlog::LogLevelNum;

const APP_ID: &str = "org.spice_gtk_usb_portal.ClientDemo";

#[derive(Clone)]
struct AppContext {
    devices: Devices,
    usbredir: Usbredir,
    attached: Rc<RefCell<HashMap<DeviceID, OwnedUsbDevice>>>,
}

fn main() -> glib::ExitCode {
    stderrlog::new()
        .module("spice_gtk_usb_portal")
        .module("client")
        .module("Spice")
        .show_module_names(true)
        .verbosity(LogLevelNum::Trace)
        .timestamp(stderrlog::Timestamp::Microsecond)
        .init()
        .unwrap();

    glib::log_set_default_handler(glib::rust_log_handler);

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(activate);
    app.run()
}

fn activate(app: &Application) {
    let app = app.clone();
    let guard = app.hold();
    glib::spawn_future_local(async move {
        let window = build_uri_window(&app);
        window.present();
        drop(guard);
    });
}

fn show_error<W: IsA<gtk::Window>>(parent: &W, msg: &str, detail: &str) {
    gtk::AlertDialog::builder()
        .modal(true)
        .message(msg)
        .detail(detail)
        .build()
        .show(Some(parent));
}

fn build_uri_window(app: &Application) -> ApplicationWindow {
    let entry = gtk::Entry::builder()
        .text("spice://127.0.0.1:5900")
        .hexpand(true)
        .build();
    let confirm = gtk::Button::builder().label("Connect").build();

    let hbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    hbox.append(&entry);
    hbox.append(&confirm);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Connection URI")
        .child(&hbox)
        .build();

    confirm.connect_clicked(clone!(
        #[strong]
        app,
        #[strong]
        window,
        #[strong]
        entry,
        move |_| {
            let uri = entry.text().to_string();
            glib::spawn_future_local(clone!(
                #[strong]
                app,
                #[strong]
                window,
                async move {
                    let display = rdw_spice::Display::new();
                    let session = display.session();
                    session.set_uri(Some(&uri));

                    let usbredir = match Usbredir::new(&session) {
                        Ok(u) => u,
                        Err(e) => {
                            error!("failed to initialize Usbredir: {e}");
                            return;
                        }
                    };

                    let devices = match Devices::new().await {
                        Ok(d) => d,
                        Err(e) => {
                            error!("failed to initialize USB portal: {e}");
                            return;
                        }
                    };

                    let ctx = AppContext {
                        devices,
                        usbredir,
                        attached: Rc::new(RefCell::new(HashMap::new())),
                    };

                    devices_window::build(&app, &ctx).present();
                    display_window::build(&app, display).present();

                    session.connect();
                    window.close();
                }
            ));
        }
    ));

    window
}

mod devices_window {
    use super::*;

    pub fn build(app: &Application, ctx: &AppContext) -> ApplicationWindow {
        let store = gio::ListStore::new::<DeviceDescription>();

        let show_hubs_check = gtk::CheckButton::builder()
            .label("Show USB hubs")
            .active(false)
            .build();

        let filter = gtk::CustomFilter::new(clone!(
            #[weak]
            show_hubs_check,
            #[upgrade_or]
            false,
            move |item| {
                let Some(desc) = item.downcast_ref::<DeviceDescription>() else {
                    return false;
                };
                show_hubs_check.is_active() || !desc.is_likely_usb_hub()
            }
        ));
        let filter_model = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));

        show_hubs_check.connect_toggled(clone!(
            #[weak]
            filter,
            move |_| {
                filter.changed(gtk::FilterChange::Different);
            }
        ));

        let channel_label = gtk::Label::builder().halign(gtk::Align::Start).build();
        channel_label.add_css_class("dim-label");
        channel_label.add_css_class("caption");
        update_channel_label(&channel_label, &ctx.usbredir);

        let listbox = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        listbox.add_css_class("boxed-list");

        let placeholder = gtk::Label::builder()
            .label("No USB devices available")
            .margin_top(24)
            .margin_bottom(24)
            .build();
        placeholder.add_css_class("dim-label");
        listbox.set_placeholder(Some(&placeholder));

        let scrolled = gtk::ScrolledWindow::builder()
            .child(&listbox)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .vexpand(true)
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        content.append(&show_hubs_check);
        content.append(&channel_label);
        content.append(&scrolled);

        let window = ApplicationWindow::builder()
            .application(app)
            .title("USB Devices")
            .default_width(480)
            .default_height(400)
            .child(&content)
            .build();

        // Bind rows after the window exists so make_row can capture it for
        // WindowIdentifier construction on Attach.
        listbox.bind_model(
            Some(&filter_model),
            clone!(
                #[strong]
                ctx,
                #[weak]
                window,
                #[upgrade_or_panic]
                move |item| {
                    let desc = item
                        .downcast_ref::<DeviceDescription>()
                        .expect("ListStore items are DeviceDescription");
                    make_row(&ctx, &window, desc).upcast()
                }
            ),
        );

        // Subscribe to add/remove before enumerating to minimise the window
        // where events between Devices::new() returning and the handlers
        // landing could be missed. The add handler de-dups by id in case
        // enumerate_devices returns an item that also arrived as an event.
        ctx.devices.connect_closure(
            "device-added",
            false,
            glib::closure_local!(
                #[weak]
                store,
                move |_: Devices, desc: DeviceDescription| {
                    if !contains_id(&store, desc.id()) {
                        store.append(&desc);
                    }
                }
            ),
        );
        let attached_for_remove = ctx.attached.clone();
        ctx.devices.connect_closure(
            "device-removed",
            false,
            glib::closure_local!(
                #[weak]
                store,
                move |_: Devices, desc: DeviceDescription| {
                    for i in 0..store.n_items() {
                        let Some(item) = store.item(i).and_downcast::<DeviceDescription>() else {
                            continue;
                        };
                        if item.id() == desc.id() {
                            store.remove(i);
                            break;
                        }
                    }
                    let owned = attached_for_remove.borrow_mut().remove(desc.id());
                    if let Some(owned) = owned {
                        glib::spawn_future_local(async move {
                            owned.detach_from_spice().await;
                        });
                    }
                }
            ),
        );
        // device-changed keeps the same DeviceID but refreshes metadata
        // (readable/writable/model/vendor). Replace the row in-place; leave
        // any currently attached OwnedUsbDevice alone — the fd is still valid.
        ctx.devices.connect_closure(
            "device-changed",
            false,
            glib::closure_local!(
                #[weak]
                store,
                move |_: Devices, desc: DeviceDescription| {
                    for i in 0..store.n_items() {
                        let Some(item) = store.item(i).and_downcast::<DeviceDescription>() else {
                            continue;
                        };
                        if item.id() == desc.id() {
                            store.splice(i, 1, &[desc]);
                            return;
                        }
                    }
                    // Device wasn't in the store yet — treat as an add.
                    store.append(&desc);
                }
            ),
        );

        glib::spawn_future_local(clone!(
            #[strong(rename_to = devices)]
            ctx.devices,
            #[weak]
            store,
            async move {
                match devices.enumerate_devices().await {
                    Ok(list) => {
                        for desc in list {
                            if !contains_id(&store, desc.id()) {
                                store.append(&desc);
                            }
                        }
                    }
                    Err(e) => warn!("enumerate_devices failed: {e}"),
                }
            }
        ));

        // Keep the channel label live.
        let update_label = clone!(
            #[weak]
            channel_label,
            move |ur: &Usbredir| {
                update_channel_label(&channel_label, ur);
            }
        );
        ctx.usbredir
            .connect_free_channels_notify(update_label.clone());
        ctx.usbredir.connect_max_channels_notify(update_label);

        window.connect_close_request(clone!(
            #[weak]
            app,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_| {
                app.quit();
                glib::Propagation::Proceed
            }
        ));

        window
    }

    fn contains_id(store: &gio::ListStore, id: &DeviceID) -> bool {
        for i in 0..store.n_items() {
            let Some(item) = store.item(i).and_downcast::<DeviceDescription>() else {
                continue;
            };
            if item.id() == id {
                return true;
            }
        }
        false
    }

    fn update_channel_label(label: &gtk::Label, usbredir: &Usbredir) {
        label.set_label(&format!(
            "USB channels: {} free / {} total",
            usbredir.free_channels(),
            usbredir.max_channels(),
        ));
    }

    fn make_row(
        ctx: &AppContext,
        parent: &ApplicationWindow,
        desc: &DeviceDescription,
    ) -> gtk::ListBoxRow {
        let vendor = desc.vendor().unwrap_or_else(|| "Unknown vendor".into());
        let model = desc.model().unwrap_or_else(|| "Unknown model".into());

        let title = gtk::Label::builder()
            .label(format!("{vendor} — {model}"))
            .halign(gtk::Align::Start)
            .build();

        let subtitle = gtk::Label::builder()
            .label(desc.id().to_string())
            .halign(gtk::Align::Start)
            .build();
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("caption");

        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        text_box.append(&title);
        text_box.append(&subtitle);

        let initially_attached = ctx.attached.borrow().contains_key(desc.id());
        let attach_button = gtk::Button::builder()
            .label(if initially_attached {
                "Detach"
            } else {
                "Attach"
            })
            .valign(gtk::Align::Center)
            .build();
        refresh_attach_sensitivity(&attach_button, ctx, desc.id(), desc.readable());

        let desc = desc.clone();
        attach_button.connect_clicked(clone!(
            #[strong]
            ctx,
            #[strong]
            desc,
            #[weak]
            parent,
            move |button| {
                glib::spawn_future_local(clone!(
                    #[strong]
                    ctx,
                    #[strong]
                    desc,
                    #[strong]
                    parent,
                    #[strong]
                    button,
                    async move {
                        let id = desc.id().clone();
                        let is_attached = ctx.attached.borrow().contains_key(&id);
                        if is_attached {
                            let owned = ctx.attached.borrow_mut().remove(&id);
                            if let Some(owned) = owned {
                                owned.detach_from_spice().await;
                                // Drop at end of scope releases the fd back to the portal.
                            }
                            button.set_label("Attach");
                            refresh_attach_sensitivity(&button, &ctx, &id, desc.readable());
                        } else {
                            if ctx.usbredir.free_channels() == 0 {
                                show_error(
                                    &parent,
                                    "No free USB channels",
                                    "The SPICE server has no free USB redirection channels.",
                                );
                                return;
                            }
                            let wid = WindowIdentifier::from_native(&parent).await;
                            let owned = match ctx
                                .devices
                                .acquire_device(wid.as_ref(), id.clone(), desc.writable())
                                .await
                            {
                                Ok(d) => d,
                                // Portal errors include user denial (Cancelled). Without
                                // access to ashpd::Error variants (not re-exported) we
                                // cannot cleanly distinguish; surface everything.
                                Err(DeviceError::Portal(e)) => {
                                    info!("portal acquire for {id} failed/denied: {e}");
                                    return;
                                }
                                Err(e) => {
                                    show_error(&parent, "Could not acquire device", &e.to_string());
                                    return;
                                }
                            };
                            if let Err(e) = ctx.usbredir.attach(&owned).await {
                                // owned drops at end of scope → portal release runs.
                                show_error(&parent, "Attach failed", &e.to_string());
                                return;
                            }
                            ctx.attached.borrow_mut().insert(id.clone(), owned);
                            button.set_label("Detach");
                            refresh_attach_sensitivity(&button, &ctx, &id, desc.readable());
                        }
                    }
                ));
            }
        ));

        // Re-evaluate sensitivity when the free-channel count changes (another
        // row attached/detached, or a usbredir channel was opened/closed).
        ctx.usbredir.connect_free_channels_notify(clone!(
            #[weak]
            attach_button,
            #[strong]
            ctx,
            #[strong]
            desc,
            move |_| {
                refresh_attach_sensitivity(&attach_button, &ctx, desc.id(), desc.readable());
            }
        ));

        let hbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        hbox.append(&text_box);
        if let Some(warning) = access_warning_icon(&desc) {
            hbox.append(&warning);
        }
        hbox.append(&attach_button);

        gtk::ListBoxRow::builder()
            .child(&hbox)
            .activatable(false)
            .build()
    }

    fn refresh_attach_sensitivity(
        button: &gtk::Button,
        ctx: &AppContext,
        id: &DeviceID,
        readable: bool,
    ) {
        let attached = ctx.attached.borrow().contains_key(id);
        button.set_sensitive(readable && (attached || ctx.usbredir.free_channels() > 0));
    }

    fn access_warning_icon(desc: &DeviceDescription) -> Option<gtk::Image> {
        let tooltip = match (desc.readable(), desc.writable()) {
            (true, true) => return None,
            (false, false) => "Device is not accessible (neither readable nor writable)",
            (true, false) => "Device is read-only",
            (false, true) => "Device is write-only (not readable)",
        };
        let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
        icon.add_css_class("warning");
        icon.set_tooltip_text(Some(tooltip));
        Some(icon)
    }
}

mod display_window {
    use super::*;

    pub fn build(app: &Application, display: rdw_spice::Display) -> ApplicationWindow {
        use rdw_spice::spice::{self, prelude::*};

        let session = display.session();
        session.connect_channel_new(move |_, channel| {
            let Ok(main) = channel.clone().downcast::<spice::MainChannel>() else {
                return;
            };
            main.connect_channel_event(|channel, event| {
                use spice::ChannelEvent::*;
                match event {
                    Opened => info!("spice main channel opened"),
                    Closed => info!("spice main channel closed"),
                    ErrorConnect | ErrorLink | ErrorTls | ErrorAuth | ErrorIo => {
                        if let Some(err) = channel.error() {
                            warn!("spice channel error ({event:?}): {err}");
                        } else {
                            warn!("spice channel error: {event:?}");
                        }
                    }
                    _ => {}
                }
            });
        });

        session.connect_disconnected(|_| {
            info!("spice session disconnected");
        });

        let window = ApplicationWindow::builder()
            .application(app)
            .title("SPICE Display")
            .default_width(1024)
            .default_height(768)
            .child(&display)
            .build();

        window.connect_close_request(clone!(
            #[weak]
            app,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_| {
                app.quit();
                glib::Propagation::Proceed
            }
        ));

        window
    }
}
