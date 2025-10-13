use crate::errors::{OtherErrorReason, ProtonError, VoidActionResult};
use crate::mail::MailSession;
use crate::{core::datatypes::DeviceEnvironment, errors::ActionError};
use proton_core_common::datatypes::RegisteredDevice as RealRegisteredDevice;
use proton_core_common::device_registration::spawn_registered_device_task;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uniffi_runtime::async_runtime;

#[derive(Clone, Debug, uniffi::Record)]
pub struct RegisteredDevice {
    /// Device token
    pub device_token: String,
    /// Environment to which we register
    pub environment: DeviceEnvironment,
    /// TODO: Document this field
    pub ping_notification_status: Option<i32>,
    /// TODO: Document this field
    pub push_notification_status: Option<i32>,
}

/// A handle to a background task responsible for registering devices.
/// Keep it in memory for as long as you wish to have registration.
/// It will abort the background task on drop.
///
/// Additionally, in order to provide device registration details,
/// this handle provides a method, [`Self::update_device`].
///
#[derive(uniffi::Object)]
pub struct RegisterDeviceTaskHandle {
    // None is used ONLY in the initial task state.
    sender: watch::Sender<Option<RealRegisteredDevice>>,
    handle: JoinHandle<()>,
}

impl Drop for RegisterDeviceTaskHandle {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[uniffi_export]
impl RegisterDeviceTaskHandle {
    /// Call this method whenever device token was received.
    ///
    #[returns(VoidActionResult)]
    pub fn update_device(&self, device: RegisteredDevice) -> Result<(), ActionError> {
        self.sender
            .send(Some(RealRegisteredDevice::from(device)))
            .map_err(|_| {
                ActionError::Other(ProtonError::OtherReason(OtherErrorReason::Other(
                    "register-device-task has crashed".into(),
                )))
            })?;

        Ok(())
    }
}

#[uniffi_export]
impl MailSession {
    /// Spawns new background task responsible for registering device for the push notification.
    /// That task will automatically watch for new sessions and register them with latest known device
    /// token.
    ///
    /// In order to provide device registration details, this function returns an object [`RegisterDeviceTaskHandle`]
    /// that has a method [`RegisterDeviceTaskHandle::update_device`].
    ///
    /// # Errors
    ///
    /// This method may fail if connection to the account database cannot be reached.
    ///
    pub fn register_device_task(&self) -> Result<Arc<RegisterDeviceTaskHandle>, ActionError> {
        async_runtime().block_on(async {
            let ctx = self.ctx().core_context().clone();
            let (tx, rx) = watch::channel(None);
            let handle = spawn_registered_device_task(ctx, rx)
                .await
                .map_err(RealProtonMailError::from)?;

            Ok(Arc::new(RegisterDeviceTaskHandle { sender: tx, handle }))
        })
    }
}

impl From<RealRegisteredDevice> for RegisteredDevice {
    fn from(value: RealRegisteredDevice) -> Self {
        Self {
            device_token: value.device_token,
            environment: value.environment.into(),
            ping_notification_status: value.ping_notification_status,
            push_notification_status: value.push_notification_status,
        }
    }
}

impl From<RegisteredDevice> for RealRegisteredDevice {
    fn from(value: RegisteredDevice) -> Self {
        Self {
            device_token: value.device_token,
            environment: value.environment.into(),
            ping_notification_status: value.ping_notification_status,
            push_notification_status: value.push_notification_status,
        }
    }
}
