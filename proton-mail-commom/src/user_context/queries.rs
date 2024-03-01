use crate::MailUserContext;
use proton_api_mail::domain::LabelType;
use proton_mail_db::proton_sqlite3::{LiveQuery, LiveQueryBuilder, ObservableQuery};
use proton_mail_db::{
    ConversationQuery, ConversationsLiveQuery, LabelsByTypeLiveQuery, LabelsByTypeQuery,
    LocalLabelId,
};

impl MailUserContext {
    pub fn new_inbox_conversations_live_query(&self) -> ConversationsLiveQuery {
        self.new_live_query(ConversationQuery::new())
    }
    pub fn new_conversation_live_query(
        &self,
        local_label_id: LocalLabelId,
    ) -> ConversationsLiveQuery {
        self.new_live_query(ConversationQuery::with_label(local_label_id))
    }

    pub fn new_labels_by_type_live_query(&self, label_type: LabelType) -> LabelsByTypeLiveQuery {
        self.new_live_query(LabelsByTypeQuery::new(label_type))
    }

    fn new_live_query<Q: ObservableQuery>(&self, q: Q) -> LiveQuery<Q> {
        LiveQueryBuilder::new(self.tracker_service().clone())
            .with_background_initializer()
            .build(q)
    }
}
