mod conversations;
mod messages;
mod model;
mod popups;

pub use model::Model;

use crate::messages::Messages;
use proton_core_common::db::proton_sqlite3::{
    InProcessTrackerService, Live, LiveQueryBuilder, Observable,
};
use proton_mail_common::db::{LocalConversationId, LocalLabelId, LocalMessageId};
use proton_mail_common::Mailbox;

const ITEM_LIMIT: usize = 50;

pub enum Message {
    Sync(Mailbox),
    OpenConversationView(Mailbox),
    OpenMessageView(Mailbox),
    OpenLabelSelectPopup,
    OpenMoveItemPopup(Item),
    OpenLabelItemPopup(Item),
    OpenUnlabelItemPopup(Item),
    SelectLabel(LocalLabelId),
    MarkConversationRead(LocalConversationId),
    MarkConversationUnread(LocalConversationId),
    DeleteConversation(LocalConversationId),
    MoveConversation(LocalConversationId, LocalLabelId),
    LabelConversation(LocalConversationId, LocalLabelId),
    UnlabelConversation(LocalConversationId, LocalLabelId),
}
pub enum Item {
    Conversation(LocalConversationId),
    //TODO:message actions
    #[allow(dead_code)]
    Message(LocalMessageId),
}

fn new_live_query<Q: Observable>(tracker: InProcessTrackerService, query: Q) -> Live<Q> {
    LiveQueryBuilder::new(tracker)
        .with_foreground_initializer()
        .build(query)
}
/*
#[derive(Clone)]
struct LabelQuery {
    label_id: LocalLabelId,
    view_mode: MailSettingsViewMode,
}

impl Observable for LabelQuery {
    type Output = Option<LocalLabelWithCount>;

    fn debug_name(&self) -> &'static str {
        "MailboxLabelObserver"
    }

    fn tables(&self) -> Vec<String> {
        if self.view_mode == MailSettingsViewMode::Conversations {
            vec!["labels".to_owned(), "label_conversation_count".to_owned()]
        } else {
            vec!["labels".to_owned(), "label_message_count".to_owned()]
        }
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection.rusqlite_connection());
        conn.label_by_type_ordered_with_message_count()
        let conversations = conn.message_metadata_list(self.label_id,
        Ok(conversations)
    }
}*/

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Mailbox(value)
    }
}
