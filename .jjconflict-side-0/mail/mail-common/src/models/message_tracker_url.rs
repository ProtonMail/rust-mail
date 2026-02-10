use crate::datatypes::LocalMessageId;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether};
use stash::{UserDb, params};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Model)]
#[TableName("message_tracker_urls")]
#[Database(UserDb)]
pub struct MessageTrackerUrl {
    #[IdField(autoincrement)]
    pub id: Option<i64>,

    #[DbField]
    pub local_message_id: LocalMessageId,

    #[DbField]
    pub tracker_domain: String,

    #[DbField]
    pub original_url: String,
}

impl MessageTrackerUrl {
    pub async fn find_by_message(
        message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Self::find(
            "WHERE local_message_id = ? ORDER BY id ASC",
            params![message_id],
            tether,
        )
        .await
    }

    pub async fn delete_by_message(
        message_id: LocalMessageId,
        tx: &Bond<'_>,
    ) -> Result<(), StashError> {
        tx.execute(
            "DELETE FROM message_tracker_urls WHERE local_message_id = ?",
            params![message_id],
        )
        .await?;
        Ok(())
    }
}
