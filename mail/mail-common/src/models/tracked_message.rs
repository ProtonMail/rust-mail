use crate::datatypes::{LocalMessageId, TrackerStatus};
use proton_core_common::datatypes::UnixTimestamp;
use stash::macros::Model;

#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("tracked_messages")]
pub struct TrackedMessage {
    #[IdField]
    pub local_message_id: LocalMessageId,

    #[DbField]
    pub status: TrackerStatus,

    #[DbField]
    pub last_checked_at: UnixTimestamp,
}
