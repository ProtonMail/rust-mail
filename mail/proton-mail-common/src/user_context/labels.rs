use crate::db::{DBResult, LabelsByTypeQueryWithConversationCount, LocalLabel, LocalLabelId};
use crate::{MailContextResult, MailUserContext, MailboxObservableQueryBuilder};
use proton_api_mail::domain::{LabelId, LabelType, ALL_LABEL_TYPES};
use proton_api_mail::proton_api_core::exports::tracing;
use proton_api_mail::proton_api_core::exports::tracing::{debug, Level};

impl MailUserContext {
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_labels(&self) -> MailContextResult<()> {
        let session = self.mail_session();

        let mut all_labels = Vec::with_capacity(64);
        for category in ALL_LABEL_TYPES {
            debug!("Fetching labels ({:?})", category);
            let labels = session.labels(category).await?;
            all_labels.extend(labels);
        }

        let mut connection = self.new_db_connection()?;
        debug!("Storing labels into database");
        connection.tx(|tx| -> DBResult<()> {
            tx.create_remote_labels(all_labels.iter())?;
            Ok(())
        })?;

        Ok(())
    }

    pub fn get_local_label_id(&self, id: &LabelId) -> MailContextResult<Option<LocalLabelId>> {
        let conn = self.new_db_connection()?;
        Ok(conn.read(|conn| conn.resolve_remote_label_id(id))?)
    }

    pub fn get_label_with_remote_id(
        &self,
        label_id: &LabelId,
    ) -> MailContextResult<Option<LocalLabel>> {
        let conn = self.new_db_connection()?;
        Ok(conn.read(|conn| conn.label_with_remote_id(label_id))?)
    }

    pub fn get_label(&self, id: LocalLabelId) -> MailContextResult<Option<LocalLabel>> {
        let conn = self.new_db_connection()?;
        Ok(conn.read(|conn| conn.label_with_id(id))?)
    }

    pub fn get_labels_by_type(&self, label_type: LabelType) -> MailContextResult<Vec<LocalLabel>> {
        let conn = self.new_db_connection()?;
        Ok(conn.read(|conn| conn.label_by_type_ordered(label_type))?)
    }

    /// Return the list of folders where messages and conversations can be moved into.
    pub fn movable_folders(&self) -> MailContextResult<Vec<LocalLabel>> {
        let conn = self.new_db_connection()?;
        Ok(conn.read(|conn| conn.labels_for_conv_or_msg_move())?)
    }

    pub fn new_system_labels_live_query<
        Builder: MailboxObservableQueryBuilder<LabelsByTypeQueryWithConversationCount>,
    >(
        &self,
        builder: Builder,
    ) -> Builder::Output {
        builder.build(
            self.tracker_service().clone(),
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
            self.tracker_service().clone(),
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
            self.tracker_service().clone(),
            LabelsByTypeQueryWithConversationCount::new(LabelType::Label),
        )
    }
}
