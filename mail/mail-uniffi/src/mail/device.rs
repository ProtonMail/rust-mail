use std::sync::Arc;

use proton_core_common::models::RegisteredDevice as RealRegisteredDevice;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

use crate::{
    core::datatypes::DeviceEnvironment,
    errors::{ActionError, VoidActionResult},
    uniffi_async,
};

use super::MailUserSession;

#[derive(Clone, Debug, uniffi::Record)]
pub struct RegisteredDevice {
    /// Device token
    pub device_token: String,
    /// Environment to which we register
    pub environment: DeviceEnvironment,
    /// PGP Public Key
    pub public_key: Option<String>,
    /// TODO: Document this field
    pub ping_notification_status: Option<i32>,
    /// TODO: Document this field
    pub push_notification_status: Option<i32>,
}

/// Return already registered device information.
///
#[proton_uniffi_macros::export_result]
pub async fn get_registered_device(
    session: Arc<MailUserSession>,
) -> Result<Option<RegisteredDevice>, ActionError> {
    uniffi_async(async move {
        let tether = session.user_stash().connection();
        let real_device = RealRegisteredDevice::get(&tether).await?;
        let device = real_device.map(From::from);
        Ok::<_, RealProtonMailError>(device)
    })
    .await
    .map_err(ActionError::from)
}

/// Register device & save the details in cache
///
#[uniffi::export]
pub async fn register_and_save_device(
    session: Arc<MailUserSession>,
    device: RegisteredDevice,
) -> VoidActionResult {
    uniffi_async(async move {
        let mut real_device = RealRegisteredDevice::from(device);
        let ctx = session.ctx();
        let mut tether = ctx.user_stash().connection();
        let tx = tether.transaction().await?;

        real_device.register(ctx.api()).await?;
        real_device.save(&tx).await?;

        tx.commit().await?;

        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(ActionError::from)
    .into()
}

impl From<RealRegisteredDevice> for RegisteredDevice {
    fn from(value: RealRegisteredDevice) -> Self {
        Self {
            device_token: value.device_token,
            environment: value.environment.into(),
            public_key: value.public_key,
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
            public_key: value.public_key,
            ping_notification_status: value.ping_notification_status,
            push_notification_status: value.push_notification_status,
            row_id: None,
        }
    }
}
