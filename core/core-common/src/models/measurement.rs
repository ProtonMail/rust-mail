use stash::{
    AccountDb,
    macros::Model,
    orm::Model,
    params,
    stash::{StashError, Tether},
};

use crate::datatypes::{LocalMeasurementId, MeasurementData, UnixTimestamp};

#[derive(Debug, Clone, PartialEq, Model)]
#[TableName("measurements")]
#[Database(AccountDb)]
pub struct Measurement {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalMeasurementId>,

    // We store everything in one field using JSON for two reasons:
    // 1. We don't really care about the content. We have no desire to index,
    // filter this data etc.
    // 2. We have to store some data in milliseconds which means u128. SQLite has
    // very limited support for u128.
    #[DbField]
    pub data: MeasurementData,

    #[DbField]
    pub created_at: UnixTimestamp,
}

impl Measurement {
    pub async fn fetch_batch(
        limit: usize,
        tx: &Tether<AccountDb>,
    ) -> Result<Vec<Self>, StashError> {
        let measurements =
            Self::find("ORDER BY created_at ASC LIMIT ?", params![limit], tx).await?;

        Ok(measurements)
    }
}
