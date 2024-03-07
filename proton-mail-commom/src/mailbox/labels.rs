use crate::{Mailbox, MailboxObservableQueryBuilder};
use proton_api_mail::domain::LabelType;
use proton_mail_db::LabelsByTypeQueryWithConversationCount;

impl<Builder: MailboxObservableQueryBuilder> Mailbox<Builder> {
    pub fn new_system_labels_live_query(
        &self,
    ) -> Builder::Type<LabelsByTypeQueryWithConversationCount> {
        self.builder.build(
            self.user_ctx.tracker_service().clone(),
            LabelsByTypeQueryWithConversationCount::new(LabelType::System),
        )
    }

    pub fn new_folder_labels_live_query(
        &self,
    ) -> Builder::Type<LabelsByTypeQueryWithConversationCount> {
        self.builder.build(
            self.user_ctx.tracker_service().clone(),
            LabelsByTypeQueryWithConversationCount::new(LabelType::System),
        )
    }

    pub fn new_label_labels_live_query(
        &self,
    ) -> Builder::Type<LabelsByTypeQueryWithConversationCount> {
        self.builder.build(
            self.user_ctx.tracker_service().clone(),
            LabelsByTypeQueryWithConversationCount::new(LabelType::System),
        )
    }
}
