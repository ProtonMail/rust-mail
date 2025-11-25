use smart_default::SmartDefault;
use stash::{
    macros::Model,
    orm::Model,
    stash::{Bond, StashError, Tether},
};

use crate::datatypes::UnixTimestamp;
use crate::models::ModelExtension;

#[derive(Debug, Clone, PartialEq, Model, SmartDefault)]
#[TableName("feature_flags")]
pub struct FeatureFlag {
    #[IdField]
    pub name: String,

    #[DbField]
    pub enabled: bool,

    #[DbField]
    pub modify_time: UnixTimestamp,
}

impl FeatureFlag {
    pub async fn by_name(
        name: impl Into<String>,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        Self::find_by_id(name.into(), tether).await
    }

    pub async fn save_all(new: Vec<Self>, tx: &Bond<'_>) -> Result<(), StashError> {
        for mut flag in new {
            Self::save(&mut flag, tx).await?;
        }

        Ok(())
    }
}
