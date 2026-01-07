use crate::datatypes::LocalMessageId;
use proton_core_common::datatypes::UnixTimestamp;
use stash::macros::Model;

#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("message_trackers")]
pub struct MessageTracker {
    #[IdField]
    pub local_message_id: LocalMessageId,

    #[DbField]
    pub last_checked_at: UnixTimestamp,
}
