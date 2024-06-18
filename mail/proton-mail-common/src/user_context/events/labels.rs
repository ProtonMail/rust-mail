use proton_api_mail::domain::LabelEvent;
use proton_api_mail::proton_api_core::domain::Action;
use proton_api_mail::proton_api_core::exports::tracing::warn;
use stash::stash::{StashError, Tether};

pub fn handle_label_events(tx: &Tether, label_events: &[LabelEvent]) -> Result<(), StashError> {
    for label_event in label_events {
        match label_event.action {
            Action::Delete => {
                tx.delete_remote_label(&label_event.id)?;
            }
            Action::Create => {
                if let Some(label) = &label_event.label {
                    tx.create_remote_label(label)?;
                } else {
                    warn!("Received label create without label");
                }
            }
            Action::Update | Action::UpdateFlags => {
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
