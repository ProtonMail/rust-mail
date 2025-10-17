use smart_default::SmartDefault;
use stash::{
    macros::Model,
    orm::Model,
    params,
    stash::{Bond, StashError, Tether},
};

use crate::datatypes::UnixTimestamp;

#[derive(Debug, Clone, PartialEq, Model, SmartDefault)]
#[TableName("feature_flags")]
pub struct FeatureFlag {
    #[IdField(autoincrement)]
    pub id: Option<u64>,

    #[DbField]
    pub name: String,

    #[DbField]
    pub enabled: bool,

    #[DbField]
    pub modify_time: UnixTimestamp,
}

impl FeatureFlag {
    pub async fn by_name(name: &str, tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::find_first("WHERE name = ?", params![name.to_owned()], tether).await
    }

    pub async fn save_all(new: Vec<Self>, tx: &Bond<'_>) -> Result<(), StashError> {
        for mut flag in new {
            Self::save(&mut flag, tx).await?;
        }

        Ok(())
    }
}
