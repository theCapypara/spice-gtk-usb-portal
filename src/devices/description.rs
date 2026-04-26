use ashpd::desktop::usb::{DeviceID, UsbDevice};
use glib;
use glib::subclass::prelude::*;
use regex::{Regex, RegexBuilder};
use std::borrow::Borrow;
use std::cell::OnceCell;
use std::sync::OnceLock;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::DeviceDescription)]
    #[derive(Debug)]
    pub struct DeviceDescription {
        pub id: OnceCell<DeviceID>,
        pub info: OnceCell<UsbDevice>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DeviceDescription {
        const NAME: &'static str = "DeviceDescription";
        type Type = super::DeviceDescription;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for DeviceDescription {}
}

glib::wrapper! {
    pub struct DeviceDescription(ObjectSubclass<imp::DeviceDescription>);
}

impl DeviceDescription {
    pub fn id(&self) -> &DeviceID {
        self.imp().id.get().unwrap()
    }

    pub fn model(&self) -> Option<String> {
        self.imp().info.get().unwrap().model()
    }

    pub fn vendor(&self) -> Option<String> {
        self.imp().info.get().unwrap().vendor()
    }

    pub fn readable(&self) -> bool {
        self.imp().info.get().unwrap().is_readable()
    }

    pub fn writable(&self) -> bool {
        self.imp().info.get().unwrap().is_writable()
    }

    pub fn parent_id(&self) -> Option<&DeviceID> {
        self.imp().info.get().unwrap().parent()
    }

    /// Check heuristically if this device is likely to be an USB hub, billboard or similiar.
    pub fn is_likely_usb_hub(&self) -> bool {
        static USB_HUB_PATTERN: OnceLock<Regex> = OnceLock::new();
        let pattern = USB_HUB_PATTERN.get_or_init(|| {
            RegexBuilder::new(
                r"^(usb)?\s*(\d.?\d?)?\s*(root)?\s*(\d.?\d?)?\s*(hub|billboard)\s*(device)?$",
            )
            .case_insensitive(true)
            .build()
            .unwrap()
        });
        let model = self.model().unwrap_or_default();
        pattern.is_match(&model)
    }
}

impl<Id: Borrow<DeviceID>, Device: Borrow<UsbDevice>> From<(Id, Device)> for DeviceDescription {
    fn from((id, info): (Id, Device)) -> Self {
        let slf: Self = glib::Object::new();
        let imp = slf.imp();
        imp.id.set(id.borrow().clone()).unwrap();
        imp.info.set(info.borrow().clone()).unwrap();
        slf
    }
}
