use crate::events::LabelEvent;
use crate::models::{ConversationCounters, MessageCounters};
use proton_core_common::events::Action;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError};
use tracing::warn;

pub async fn handle_label_events(
    tx: &Bond<'_>,
    label_events: &[LabelEvent],
) -> Result<(), StashError> {
    for label_event in label_events {
        label_event.action.log_entry(&label_event.remote_id);
        match label_event.action {
            Action::Delete => {
                tx.execute(
                    "DELETE FROM labels WHERE remote_id = ?",
                    params![label_event.remote_id.clone()],
                )
                .await?;
            }
            Action::Create => {
                if let Some(mut label) = label_event.label.clone() {
                    label.save(tx).await?;
                    let label_id = label.id();
                    MessageCounters::new(label_id).save(tx).await?;
                    ConversationCounters::new(label_id).save(tx).await?;
                } else {
                    warn!("Received label create without label");
                }
            }
            Action::Update | Action::UpdateFlags => {
                if let Some(mut label) = label_event.label.clone() {
                    label.save(tx).await?;
                } else {
                    warn!("Received label update without label");
                }
            }
        }
    }
    Ok(())
}
