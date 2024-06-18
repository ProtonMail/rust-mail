use proton_api_mail::domain::ConversationEvent;
use proton_api_mail::proton_api_core::domain::Action;
use proton_api_mail::proton_api_core::exports::tracing::warn;
use stash::stash::{StashError, Tether};

pub fn handle_conversation_events(
    tx: &Tether,
    conversation_events: &[ConversationEvent],
) -> Result<(), StashError> {
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
