use crate::datatypes::LocalMessageId;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};

#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("tracking_urls")]
pub struct TrackingUrl {
    #[IdField(autoincrement)]
    pub id: Option<i64>,

    #[DbField]
    pub local_message_id: LocalMessageId,

    #[DbField]
    pub tracker_domain: String,

    #[DbField]
    pub original_url: String,
}

impl TrackingUrl {
    pub async fn find_by_message(
        message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Self::find("WHERE local_message_id = ?", params![message_id], tether).await
    }

    pub async fn delete_by_message(
        message_id: LocalMessageId,
        tx: &Bond<'_>,
    ) -> Result<(), StashError> {
        tx.execute(
            "DELETE FROM tracking_urls WHERE local_message_id = ?",
            params![message_id],
        )
        .await?;
        Ok(())
    }
}
