use crate::{MailContextResult, MailUserContext};
use proton_api_mail::domain::{LabelId, LabelType, ALL_LABEL_TYPES};
use proton_api_mail::proton_api_core::exports::tracing;
use proton_api_mail::proton_api_core::exports::tracing::{debug, Level};
use proton_mail_db::{DBResult, LocalLabel, LocalLabelId, LocalLabelWithCount};

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
        connection.tx(|tx| -> DBResult<()> { tx.create_remote_labels(all_labels.iter()) })?;

        Ok(())
    }

    pub fn get_local_label_id(&self, id: &LabelId) -> MailContextResult<Option<LocalLabelId>> {
        let conn = self.new_db_connection()?;
        let id = conn.as_connection_ref().resolve_remote_label_id(id)?;
        Ok(id)
    }

    pub fn get_label_with_remote_id(
        &self,
        label_id: &LabelId,
    ) -> MailContextResult<Option<LocalLabel>> {
        let conn = self.new_db_connection()?;
        let r = conn
            .as_connection_ref()
            .get_local_label_with_remote_id(label_id)?;
        Ok(r)
    }

    pub fn get_label(&self, id: LocalLabelId) -> MailContextResult<Option<LocalLabel>> {
        let conn = self.new_db_connection()?;
        let r = conn.as_connection_ref().get_local_label(id)?;
        Ok(r)
    }

    pub fn get_labels_by_type(
        &self,
        label_type: LabelType,
    ) -> MailContextResult<Vec<LocalLabelWithCount>> {
        let conn = self.new_db_connection()?;
        let r = conn
            .as_connection_ref()
            .get_local_label_by_type_ordered_with_conversation_count(label_type)?;
        Ok(r)
    }
}
