use proton_api_mail::domain::ConversationEvent;
use proton_api_mail::proton_api_core::domain::EventAction;
use proton_api_mail::proton_api_core::exports::tracing::warn;
use proton_mail_db::{DBResult, MailSqliteConnectionMut};

pub fn handle_conversation_events(
    tx: &mut MailSqliteConnectionMut,
    conversation_events: &[ConversationEvent],
) -> DBResult<()> {
    for conversation_event in conversation_events {
        match conversation_event.action {
            EventAction::Delete => {
                tx.mark_remote_conversation_as_deleted(&conversation_event.id)?;
            }
            EventAction::Create => {
                if let Some(conversation) = &conversation_event.conversation {
                    tx.create_conversation(conversation)?;
                } else {
                    warn!("Received create without conversation");
                }
            }
            EventAction::Update | EventAction::UpdateFlags => {
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
