#[cfg(test)]
#[path = "../tests/models/device.rs"]
mod tests;

use std::sync::Arc;

use proton_api_core::{
    service::ApiServiceError,
    services::proton::{prelude::RegisterDeviceRequest, ProtonCore},
};
use stash::{
    macros::Model,
    orm::Model,
    stash::{Bond, StashError, Tether},
};

use crate::{datatypes::DeviceEnvironment, Context};

/// Error encountered during operatin on registered device model
///
#[derive(Debug, thiserror::Error)]
pub enum RegisteredDeviceError {
    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),

    #[error("Failed to generate device key pair")]
    Crypto,
}

/// This model is used to registed the device for Push notifications.
///
/// Note, that in the database at the same time there should be only one row in `registered_devices`.
/// It is because there should be only one session for one app.
///
#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("registered_devices")]
pub struct RegisteredDevice {
    /// Device token, used as primary key
    #[IdField]
    pub device_token: String,

    /// Environment to which we register
    #[DbField]
    pub environment: DeviceEnvironment,

    //// PGP Public Key
    #[DbField]
    pub public_key: Option<String>,

    /// TODO: Document this field
    #[DbField]
    pub ping_notification_status: Option<i32>,

    /// TODO: Document this field
    #[DbField]
    pub push_notification_status: Option<i32>,

    /// The internal row ID of the record in the database. This is assigned by
    /// `SQLite`, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl RegisteredDevice {
    /// Returns last registered device if it does exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails
    ///
    pub async fn get(tether: &Tether) -> Result<Option<Self>, StashError> {
        // There should be always max one registered device in the table
        // The order by logic is an extra failsafe. If for any reason there are more than two rows in the table,
        // we will always return the latest one, guaranteeing at least some kind of consistency.
        Self::find_first("ORDER BY rowid DESC", vec![], tether).await
    }

    /// Registers the device for Push Notifications.
    ///
    /// # Errors
    ///
    /// Returns an error if the API call fails
    ///
    pub async fn register<API: ProtonCore>(&self, api: &API) -> Result<(), ApiServiceError> {
        api.register_device(RegisterDeviceRequest {
            device_token: self.device_token.clone(),
            environment: self.environment.into(),
            public_key: self.public_key.clone(),
            ping_notification_status: self.ping_notification_status,
            push_notification_status: self.push_notification_status,
        })
        .await?;
        Ok(())
    }

    /// Save or update a registered device.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is updated correctly in the database.
    ///
    /// This method ensures that there is only one registered device in the table.
    /// Otherwise, it overwrites old record.
    ///
    /// If public key does not exist in the record, it generates a new key pair, stores private part in keychain and then
    /// saves such a model to DB.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails
    ///
    pub async fn save(
        &mut self,
        bond: &Bond<'_>,
        ctx: &Arc<Context>,
    ) -> Result<(), RegisteredDeviceError> {
        // Make sure there will be only one row.
        if let Some(existing) = Self::get(bond).await? {
            self.row_id = existing.row_id;
            self.public_key = existing.public_key;
        }

        if self.public_key.is_none() {
            let pgp_provider = proton_crypto::new_pgp_provider();

            let new_key = ctx
                .gen_device_key_pair(&pgp_provider)
                .map_err(|_| RegisteredDeviceError::Crypto)?;

            self.public_key = Some(new_key.into());
        }

        <Self as Model>::save(self, bond).await?;

        Ok(())
    }
}
