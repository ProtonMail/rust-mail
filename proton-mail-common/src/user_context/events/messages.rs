use proton_api_mail::domain::MessageEvent;
use proton_api_mail::proton_api_core::domain::EventAction;
use proton_api_mail::proton_api_core::exports::tracing::warn;
use proton_mail_db::{DBResult, MailSqliteConnectionMut};

pub fn handle_message_events(
    tx: &mut MailSqliteConnectionMut,
    message_events: &[MessageEvent],
) -> DBResult<()> {
    for message_event in message_events {
        match message_event.action {
            EventAction::Delete => {
                tx.delete_remote_message(&message_event.id)?;
            }
            EventAction::Create => {
                if let Some(message) = &message_event.message {
                    tx.create_message_from_metadata(message)?;
                } else {
                    warn!("Received create message without message");
                }
            }
            EventAction::Update | EventAction::UpdateFlags => {
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
