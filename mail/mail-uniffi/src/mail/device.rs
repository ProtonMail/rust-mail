use crate::errors::VoidActionResult;
use crate::mail::MailSession;
use crate::{core::datatypes::DeviceEnvironment, errors::ActionError, uniffi_async};
use proton_core_common::models::{
    RegisteredDevice as RealRegisteredDevice, spawn_registered_device_task,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::errors::unexpected::Unexpected;
use proton_task_service::AsyncTaskResult;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

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

#[derive(uniffi::Object)]
pub struct RegisterDeviceTaskHandle {
    // None is used ONLY in the initial task state.
    sender: watch::Sender<Option<RealRegisteredDevice>>,
    handle: JoinHandle<AsyncTaskResult<()>>,
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
    async fn update_device(&self, device: RegisteredDevice) -> Result<(), ActionError> {
        self.sender
            .send(Some(RealRegisteredDevice::from(device)))
            .map_err(|_| Unexpected::Internal)
            .map_err(|e| RealProtonMailError::Unexpected(e))?;
        Ok(())
    }
}

#[uniffi_export]
pub async fn register_device_task(
    session: Arc<MailSession>,
) -> Result<Arc<RegisterDeviceTaskHandle>, ActionError> {
    uniffi_async(async move {
        let (tx, rx) = watch::channel(None);
        let ctx = session.ctx().core_context().clone();

        let handle = spawn_registered_device_task(ctx, rx).await?;

        Ok::<_, RealProtonMailError>(Arc::new(RegisterDeviceTaskHandle { sender: tx, handle }))
    })
    .await
    .map_err(ActionError::from)
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
            row_id: None,
        }
    }
}
