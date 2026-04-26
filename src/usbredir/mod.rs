mod error;

use spice_client_glib as spice;
use std::cell::Cell;
use std::os::fd::AsRawFd;

pub use self::error::*;

use crate::devices::OwnedUsbDevice;
use glib::prelude::*;
use glib::subclass::prelude::*;
use log::trace;

mod imp {
    use super::*;

    #[derive(Default, Debug, glib::Properties)]
    #[properties(wrapper_type = super::Usbredir)]
    pub struct Usbredir {
        pub spice_usb_manager: glib::WeakRef<spice::UsbDeviceManager>,
        #[property(get)]
        pub free_channels: Cell<u32>,
        #[property(get)]
        pub max_channels: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Usbredir {
        const NAME: &'static str = "Usbredir";
        type Type = super::Usbredir;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Usbredir {
        fn dispose(&self) {
            trace!("Dispose Usbredir @ {self:?}");
        }
    }

    impl Drop for Usbredir {
        fn drop(&mut self) {
            trace!("Drop Usbredir @ {self:?}");
        }
    }
}

glib::wrapper! {
    pub struct Usbredir(ObjectSubclass<imp::Usbredir>);
}

impl Usbredir {
    /// Initialize USB redirection for the given SPICE session.
    pub fn new(session: &spice::Session) -> UsbredirResult<Self> {
        let slf: Self = glib::Object::new();
        let imp = slf.imp();

        let usb_manager = spice::UsbDeviceManager::get(session)?;
        imp.spice_usb_manager.set(Some(&usb_manager));

        // spice-gtk's usb manager does not reliably notify about free-channels changes,
        // so we refresh the counts whenever a UsbredirChannel is added or removed.
        let refresh = glib::clone!(
            #[weak]
            slf,
            move |session: &spice::Session, channel: &spice::Channel| {
                if !channel.is::<spice::UsbredirChannel>() {
                    return;
                }
                let session = session.clone();
                glib::idle_add_local_once(glib::clone!(
                    #[weak]
                    slf,
                    move || {
                        let Some(manager) = slf.imp().spice_usb_manager.upgrade() else {
                            return;
                        };
                        slf.refresh_channel_counts(&session, &manager);
                    }
                ));
            }
        );
        session.connect_channel_new(refresh.clone());
        session.connect_channel_destroy(refresh);

        slf.refresh_channel_counts(session, &usb_manager);

        Ok(slf)
    }

    pub async fn attach(&self, device: &mut OwnedUsbDevice) -> UsbredirResult<()> {
        let Some(usb_manager) = self.imp().spice_usb_manager.upgrade() else {
            return Err(UsbredirError::NotConnected);
        };
        trace!("attaching device");

        if device.attached() {
            device.detach_from_spice().await;
        }

        let Some(spice_device) =
            usb_manager.allocate_device_for_file_descriptor(device.as_raw_fd())?
        else {
            return Err(UsbredirError::DeviceAttachFailed);
        };
        device.release_fd();

        trace!("spice device created");

        usb_manager.can_redirect_device(&spice_device)?;
        usb_manager.connect_device_future(&spice_device).await?;

        trace!("device connected");

        device.spice_device = Some((usb_manager.clone(), spice_device));

        if let Some(session) = usb_manager.session() {
            self.refresh_channel_counts(&session, &usb_manager);
        }

        Ok(())
    }

    fn refresh_channel_counts(&self, session: &spice::Session, manager: &spice::UsbDeviceManager) {
        let imp = self.imp();

        let max = session
            .channels()
            .iter()
            .filter(|c| c.is::<spice::UsbredirChannel>())
            .count() as u32;
        let free = manager.free_channels().max(0) as u32;

        if imp.max_channels.get() != max {
            imp.max_channels.set(max);
            self.notify_max_channels();
        }
        if imp.free_channels.get() != free {
            imp.free_channels.set(free);
            self.notify_free_channels();
        }
    }
}
