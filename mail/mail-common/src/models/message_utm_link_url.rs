use crate::datatypes::LocalMessageId;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{StashError, Tether};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Model)]
#[TableName("message_utm_link_urls")]
pub struct MessageUtmLinkUrl {
    #[IdField(autoincrement)]
    pub id: Option<i64>,

    #[DbField]
    pub local_message_id: LocalMessageId,

    #[DbField]
    pub original_url: String,

    #[DbField]
    pub cleaned_url: String,
}

impl MessageUtmLinkUrl {
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
}
