use stash::{
    macros::Model,
    orm::Model,
    params,
    stash::{Bond, StashError, Tether},
};

use crate::datatypes::{UnixTimestamp, UserFeatureFlagSource};

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
    pub overrided_value: Option<bool>,

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
            overrided_value: None,
            modify_time,
        }
    }

    #[must_use]
    pub fn legacy(
        name: impl Into<String>,
        enabled: bool,
        writable: bool,
        modify_time: UnixTimestamp,
    ) -> Self {
        Self {
            name: name.into(),
            enabled,
            source: UserFeatureFlagSource::Legacy,
            writable,
            overrided_value: None,
            modify_time,
        }
    }

    pub async fn by_name(
        name: impl Into<String>,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        let name: String = name.into();
        Self::find_first(
            // If there are two flags with the same name, use Unleash one.
            "WHERE name = ? ORDER BY source ASC",
            params![name],
            tether,
        )
        .await
    }

    pub async fn save_all(new: Vec<Self>, tx: &Bond<'_>) -> Result<(), StashError> {
        for mut flag in new {
            Self::save(&mut flag, tx).await?;
        }

        Ok(())
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.overrided_value.unwrap_or(self.enabled)
    }
}
