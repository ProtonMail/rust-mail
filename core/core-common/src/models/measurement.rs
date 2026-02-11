use stash::{
    AccountDb,
    macros::Model,
    orm::Model,
    params,
    stash::{Bond, StashError},
    utils::{IterMapToSql as _, placeholders},
};

use crate::datatypes::{MeasurementData, UnixTimestamp};

#[derive(Debug, Clone, PartialEq, Model)]
#[TableName("measurements")]
#[Database(AccountDb)]
pub struct Measurement {
    #[IdField(autoincrement)]
    pub local_id: Option<i64>,

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
    pub async fn take_batch(
        limit: usize,
        tx: &Bond<'_, AccountDb>,
    ) -> Result<Vec<Self>, StashError> {
        let measurements =
            Self::find("ORDER BY created_at ASC LIMIT ?", params![limit], tx).await?;

        if !measurements.is_empty() {
            let ids = measurements.iter().map(|i| i.local_id).bridge_sql();
            let placeholders = placeholders(&ids);

            tx.execute(
                format!("DELETE FROM measurements WHERE local_id IN ({placeholders})"),
                ids,
            )
            .await?;
        }

        Ok(measurements)
    }

    pub async fn clear_all(tx: &Bond<'_, AccountDb>) -> Result<(), StashError> {
        tx.execute("DELETE FROM measurements", params![]).await?;
        Ok(())
    }
}
