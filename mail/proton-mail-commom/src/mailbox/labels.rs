use crate::{Mailbox, MailboxObservableQueryBuilder, MailboxResult};
use proton_api_mail::domain::LabelType;
use proton_mail_db::{LabelsByTypeQueryWithConversationCount, LocalLabelWithCount};

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
            LabelsByTypeQueryWithConversationCount::new(LabelType::Folder),
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
            LabelsByTypeQueryWithConversationCount::new(LabelType::Label),
        )
    }

    pub fn get_labels_by_type(
        &self,
        label_type: LabelType,
    ) -> MailboxResult<Vec<LocalLabelWithCount>> {
        let v = self.user_ctx.get_labels_by_type(label_type)?;
        Ok(v)
    }
}
