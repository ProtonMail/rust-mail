use crate::db::{DBResult, MailSqliteConnectionMut};
use proton_api_mail::domain::LabelEvent;
use proton_api_mail::proton_api_core::domain::EventAction;
use proton_api_mail::proton_api_core::exports::tracing::warn;

pub fn handle_label_events(
    tx: &mut MailSqliteConnectionMut,
    label_events: &[LabelEvent],
) -> DBResult<()> {
    for label_event in label_events {
        match label_event.action {
            EventAction::Delete => {
                tx.delete_remote_label(&label_event.id)?;
            }
            EventAction::Create => {
                if let Some(label) = &label_event.label {
                    tx.create_remote_label(label)?;
                } else {
                    warn!("Received label create without label");
                }
            }
            EventAction::Update | EventAction::UpdateFlags => {
                if let Some(label) = &label_event.label {
                    tx.update_remote_label(label)?;
                } else {
                    warn!("Received label update without label");
                }
            }
        }
    }
    Ok(())
}
