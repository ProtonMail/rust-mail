use proton_api_mail::domain::MessageEvent;
use proton_api_mail::proton_api_core::domain::Action;
use proton_api_mail::proton_api_core::exports::tracing::warn;
use stash::stash::{StashError, Tether};

pub fn handle_message_events(
    tx: &Tether,
    message_events: &[MessageEvent],
) -> Result<(), StashError> {
    for message_event in message_events {
        match message_event.action {
            Action::Delete => {
                tx.delete_remote_message(&message_event.id)?;
            }
            Action::Create => {
                if let Some(message) = &message_event.message {
                    tx.create_message_from_metadata(message)?;
                } else {
                    warn!("Received create message without message");
                }
            }
            Action::Update | Action::UpdateFlags => {
                if let Some(message) = &message_event.message {
                    tx.update_message_from_metadata(message)?;
                } else {
                    warn!("Received update message without label");
                }
            }
        }
    }
    Ok(())
}
