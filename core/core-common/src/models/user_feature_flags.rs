use stash::{
    macros::Model,
    orm::Model,
    stash::{Bond, StashError, Tether},
};

use crate::datatypes::{UnixTimestamp, UserFeatureFlagSource};
use crate::models::ModelExtension;

#[derive(Debug, Clone, PartialEq, Model)]
#[TableName("user_feature_flags")]
pub struct UserFeatureFlag {
    #[IdField]
    pub name: String,

    #[DbField]
    pub enabled: bool,

    #[DbField]
    pub source: UserFeatureFlagSource,

    #[DbField]
    pub writable: bool,

    #[DbField]
    pub r#override: Option<bool>,

    #[DbField]
    pub modify_time: UnixTimestamp,
}

impl UserFeatureFlag {
    #[must_use]
    pub fn unleash(name: impl Into<String>, modify_time: UnixTimestamp) -> Self {
        Self {
            name: name.into(),
            enabled: true,
            source: UserFeatureFlagSource::Unleash,
            writable: false,
            r#override: None,
            modify_time,
        }
    }

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

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.r#override.unwrap_or(self.enabled)
    }
}
