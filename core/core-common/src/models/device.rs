use proton_api_core::{
    service::ApiServiceError,
    services::proton::{prelude::RegisterDeviceRequest, ProtonCore},
};
use stash::{
    macros::Model,
    orm::Model,
    stash::{Bond, StashError, Tether},
};

use crate::datatypes::DeviceEnvironment;

/// This model is used to registed the device for Push notifications.
///
/// Note, that in the database at the same time there should be only one row in `registered_devices`.
/// It is because there should be only one session for one app.
///
#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("registered_devices")]
pub struct RegisteredDevice {
    /// The local ID of the record. Note, that it is required by Stash, but since
    /// we do not have remote id counterpart it is not very useful.
    #[IdField(autoincrement)]
    pub local_id: Option<u64>,

    /// Device token
    #[DbField]
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
        Self::find_first("", vec![], tether).await
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
    /// Otherwise, it removes old record.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        // Make sure there will be only one row.
        // Remove ALL other rows
        bond.execute(format!("DELETE FROM {}", Self::table_name()), vec![])
            .await?;
        <Self as Model>::save(self, bond).await
    }
}
