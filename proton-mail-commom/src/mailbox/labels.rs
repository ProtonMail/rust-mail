use crate::{Mailbox, MailboxObservableQueryBuilder};
use proton_api_mail::domain::LabelType;
use proton_mail_db::LabelsByTypeQueryWithConversationCount;

impl Mailbox {
    pub fn new_system_labels_live_query<
        Builder: MailboxObservableQueryBuilder<LabelsByTypeQueryWithConversationCount>,
    >(
        &self,
        builder: Builder,
    ) -> Builder::Output {
        builder.build(
            self.user_ctx.tracker_service().clone(),
            LabelsByTypeQueryWithConversationCount::new(LabelType::System),
        )
    }

    pub fn new_folder_labels_live_query<
        Builder: MailboxObservableQueryBuilder<LabelsByTypeQueryWithConversationCount>,
    >(
        &self,
        builder: Builder,
    ) -> Builder::Output {
        builder.build(
            self.user_ctx.tracker_service().clone(),
            LabelsByTypeQueryWithConversationCount::new(LabelType::System),
        )
    }

    pub fn new_label_labels_live_query<
        Builder: MailboxObservableQueryBuilder<LabelsByTypeQueryWithConversationCount>,
    >(
        &self,
        builder: Builder,
    ) -> Builder::Output {
        builder.build(
            self.user_ctx.tracker_service().clone(),
            LabelsByTypeQueryWithConversationCount::new(LabelType::System),
        )
    }
}
