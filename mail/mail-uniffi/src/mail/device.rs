use crate::errors::VoidActionResult;
use crate::mail::{MailSession, MailUserSession};
use crate::{core::datatypes::DeviceEnvironment, errors::ActionError, uniffi_async};
use proton_core_common::models::RegisteredDevice as RealRegisteredDevice;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use std::sync::Arc;

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

/// Return already registered device information.
///
/// # Session
///
/// Note, this function can be executed before logging in. It loads
/// the device token from shared account database
///
#[uniffi_export]
pub async fn get_registered_device(
    session: Arc<MailSession>,
) -> Result<Option<RegisteredDevice>, ActionError> {
    uniffi_async(async move {
        let tether = session.session_stash().connection();
        let real_device = RealRegisteredDevice::get(&tether).await?;
        let device = real_device.map(From::from);
        Ok::<_, RealProtonMailError>(device)
    })
    .await
    .map_err(ActionError::from)
}

/// Register and save device into the database.
///
/// It also generates public and private device key (storing it in keychain) the first time its needed.
///
/// # Session
///
/// This function can be only executed after logging in. If you just want to store device token for
/// the sake of registering it later, use [`save_registered_device`] instead.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn register_and_save_device(
    session: Arc<MailUserSession>,
    device: RegisteredDevice,
) -> Result<(), ActionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let mut real_device = RealRegisteredDevice::from(device);

        let mut tether = ctx
            .mail_context()
            .core_context()
            .account_stash()
            .connection();

        tether
            .tx(async |tx| {
                real_device
                    .save(&tx, ctx.mail_context().core_context())
                    .await
            })
            .await?;

        real_device.register(ctx.api()).await?;

        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Save device details in cache.
///
/// # Session
///
/// Note, this function can be executed before logging in. It stores
/// the device token in shared account database. If you already have user session,
/// you probably should use [`register_and_save_device`] instead.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn save_registered_device(
    session: Arc<MailSession>,
    device: RegisteredDevice,
) -> Result<(), ActionError> {
    uniffi_async(async move {
        let mut real_device = RealRegisteredDevice::from(device);
        let mut tether = session.ctx().session_stash().connection();
        tether
            .tx(async |tx| real_device.save(&tx, session.ctx().core_context()).await)
            .await?;

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
            public_key: None,
            ping_notification_status: value.ping_notification_status,
            push_notification_status: value.push_notification_status,
            row_id: None,
        }
    }
}
