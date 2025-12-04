use stash::{
    macros::Model,
    orm::Model,
    params,
    stash::{Bond, StashError, Tether},
    utils::{IterMapToSql, placeholders_n},
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
    pub overridden_to: Option<bool>,

    #[DbField]
    pub overridden_at: Option<UnixTimestamp>,

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
            overridden_to: None,
            overridden_at: None,
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
            overridden_to: None,
            overridden_at: None,
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

    pub async fn delete_batch_from_source(
        names: Vec<String>,
        source: UserFeatureFlagSource,
        tx: &Bond<'_>,
    ) -> Result<(), StashError> {
        tx.execute(
            format!(
                "DELETE FROM {} WHERE name IN ({}) AND source = ?",
                Self::table_name(),
                placeholders_n(names.len())
            ),
            names.bridge_sql_iter().chain(params![source]).collect(),
        )
        .await?;
        Ok(())
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.overridden_to.unwrap_or(self.enabled)
    }
}
