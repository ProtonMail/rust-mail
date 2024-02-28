use crate::state::DataLoadError;
use proton_mail_db::LocalLabel;

#[derive(Debug)]
pub enum MailboxEvents {
    LoadLabels(Result<Vec<LocalLabel>, DataLoadError>),
    LoadConversations(Result<(), DataLoadError>),
}
