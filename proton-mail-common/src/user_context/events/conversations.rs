use crate::db::{DBResult, MailSqliteConnectionMut};
use proton_api_mail::domain::ConversationEvent;
use proton_api_mail::proton_api_core::domain::Action;
use proton_api_mail::proton_api_core::exports::tracing::warn;

pub fn handle_conversation_events(
    tx: &mut MailSqliteConnectionMut,
    conversation_events: &[ConversationEvent],
) -> DBResult<()> {
    for conversation_event in conversation_events {
        match conversation_event.action {
            Action::Delete => {
                tx.delete_remote_conversation(&conversation_event.id)?;
            }
            Action::Create => {
                if let Some(conversation) = &conversation_event.conversation {
                    tx.create_conversation(conversation)?;
                } else {
                    warn!("Received create without conversation");
                }
            }
            Action::Update | Action::UpdateFlags => {
                if let Some(conversation) = &conversation_event.conversation {
                    tx.update_conversation(conversation)?;
                } else {
                    warn!("Received update without conversation");
                }
            }
        }
    }
    Ok(())
}
