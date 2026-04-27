mod description;
mod error;
mod opened_device;

pub use self::description::DeviceDescription;
pub use self::error::*;
pub use self::opened_device::OwnedUsbDevice;
use super::{DeviceID, WindowIdentifier};
use ashpd::desktop::Session as PortalSession;
use ashpd::desktop::usb::{Device, UsbError, UsbEventAction, UsbProxy};
use futures_util::{FutureExt, StreamExt, select};
use gio::prelude::*;
use gio::subclass::prelude::*;
use glib;
use glib::Priority;
use glib::subclass::Signal;
use glib::timeout_future;
use log::{debug, trace, warn};
use std::cell::OnceCell;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

mod imp {
    use super::*;

    #[derive(Default, Debug)]
    pub struct Devices {
        pub proxy: OnceCell<Arc<UsbProxy>>, // UsbProxy does not implement Clone for some reason?
        pub session: OnceCell<PortalSession<UsbProxy>>,
    }

    unsafe impl ClassStruct for crate::ffi::SpiceUsbPortalDevicesClass {
        type Type = Devices;
    }

    unsafe impl InstanceStruct for crate::ffi::SpiceUsbPortalDevices {
        type Type = Devices;
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Devices {
        const NAME: &'static str = "SpiceUsbPortalDevices";
        type Type = super::Devices;
        type ParentType = glib::Object;
        type Interfaces = (gio::AsyncInitable,);
        type Class = crate::ffi::SpiceUsbPortalDevicesClass;
        type Instance = crate::ffi::SpiceUsbPortalDevices;
    }

    impl ObjectImpl for Devices {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("device-added")
                        .param_types([DeviceDescription::static_type()])
                        .build(),
                    Signal::builder("device-removed")
                        .param_types([DeviceDescription::static_type()])
                        .build(),
                    Signal::builder("device-changed")
                        .param_types([DeviceDescription::static_type()])
                        .build(),
                ]
            })
        }

        fn dispose(&self) {
            trace!("Dispose Devices @ {self:?}");
        }
    }

    impl AsyncInitableImpl for Devices {
        fn init_future(
            &self,
            _io_priority: Priority,
        ) -> Pin<Box<dyn Future<Output = std::result::Result<(), glib::Error>> + 'static>> {
            let this = self.obj().clone();
            Box::pin(async move {
                debug!("init Devices @ {this:?}");
                let imp = this.imp();
                let usb = UsbProxy::new().await.map_err(map_init_err)?;
                trace!("usb proxy created @ {this:?}");
                let session = usb
                    .create_session(Default::default())
                    .await
                    .map_err(map_init_err)?;
                trace!("session created @ {this:?}");

                imp.proxy
                    .set(Arc::new(usb))
                    .expect("object can only be init once");
                imp.session
                    .set(session)
                    .expect("object can only be init once");

                let this_weak = this.downgrade();
                glib::spawn_future_local(Box::pin(async move {
                    loop {
                        let Some(this) = this_weak.upgrade() else {
                            return;
                        };
                        let imp = this.imp();

                        let mut usb_event_fut =
                            Box::pin(imp.proxy.get().unwrap().receive_device_events().fuse());
                        // check every 10 seconds that the object is still alive
                        // (drop strong ref, try to get weak ref again)
                        let mut sleep_fut = timeout_future(Duration::from_secs(10)).fuse();
                        trace!("check events @ {this:?}");

                        let res = select! {
                            res = usb_event_fut => res,
                            _ = sleep_fut => continue,
                        };
                        trace!("got events @ {this:?}");

                        match res {
                            Ok(mut stream) => {
                                if let Some(response) = stream.next().await {
                                    let events = response.events();
                                    for ev in events {
                                        let action = ev.action();
                                        let device_id = ev.device_id();
                                        let device = ev.device();
                                        debug!(
                                            "Received event: {:#?} for device {} ({:?}, {:?})",
                                            action,
                                            device_id,
                                            device.vendor(),
                                            device.model(),
                                        );
                                        let description =
                                            DeviceDescription::from((device_id, device));
                                        match ev.action() {
                                            UsbEventAction::Add => {
                                                this.emit_by_name::<()>(
                                                    "device-added",
                                                    &[&description],
                                                );
                                            }
                                            UsbEventAction::Change => {
                                                this.emit_by_name::<()>(
                                                    "device-changed",
                                                    &[&description],
                                                );
                                            }
                                            UsbEventAction::Remove => {
                                                this.emit_by_name::<()>(
                                                    "device-removed",
                                                    &[&description],
                                                );
                                            }
                                        }
                                    }
                                } else {
                                    debug!("event list was empty");
                                }
                            }
                            Err(err) => {
                                warn!("usb device watching failed - retrying in 10sec: {err:?}");
                                timeout_future(Duration::from_secs(10)).await;
                            }
                        }
                    }
                }));

                trace!("init done @ {this:?}");
                Ok(())
            })
        }
    }

    impl Drop for Devices {
        fn drop(&mut self) {
            trace!("Drop Devices @ {self:?}");
        }
    }

    fn map_init_err(err: ashpd::Error) -> glib::Error {
        glib::Error::new(crate::Error::Portal, &err.to_string())
    }
}

glib::wrapper! {
    pub struct Devices(ObjectSubclass<imp::Devices>) @implements gio::AsyncInitable;
}

impl Devices {
    pub async fn new() -> DeviceResult<Self> {
        let slf: Self = gio::AsyncInitable::new_future(Priority::DEFAULT)
            .await
            .map_err(DeviceError::Init)?;
        debug!("new Devices @ {slf:?}");
        Ok(slf)
    }

    pub async fn acquire_device(
        &self,
        parent_window: Option<&WindowIdentifier>,
        device_id: &DeviceID,
        writable: bool,
    ) -> DeviceResult<OwnedUsbDevice> {
        let usb = self.imp().proxy.get().expect("object must be init");
        let acquired = usb
            .acquire_devices(
                parent_window,
                &[Device::new(device_id.clone(), writable)],
                Default::default(),
            )
            .await?;
        for (acquired_id, result) in acquired {
            if acquired_id == *device_id {
                let owned_device = OwnedUsbDevice::new(usb, acquired_id, result?);
                debug!("opened usb device {owned_device:?}");
                return Ok(owned_device);
            }
        }
        Err(UsbError(Some("portal did not return the expected device".into())).into())
    }

    pub async fn enumerate_devices(&self) -> DeviceResult<Vec<DeviceDescription>> {
        let usb = self.imp().proxy.get().expect("object must be init");
        Ok(usb
            .enumerate_devices(Default::default())
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }
}
