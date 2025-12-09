use crate::AppError;
use crate::models::{ConversationCounters, MessageCounters};
use proton_core_common::event_loop::events::{Action, LabelEvent};
use proton_core_common::models::{Label, ModelIdExtension};
use stash::orm::Model;
use stash::stash::Bond;

pub async fn handle_label_events(
    tx: &Bond<'_>,
    label_events: &[LabelEvent],
) -> Result<(), AppError> {
    for label_event in label_events {
        if label_event.action == Action::Create {
            tracing::info!(
                "Creating message and conversation counters for {}",
                label_event.remote_id
            );
            let local_id = Label::remote_id_counterpart(label_event.remote_id.clone(), tx)
                .await?
                .ok_or(AppError::RemoteLabelDoesNotExist(
                    label_event.remote_id.clone(),
                ))?;
            MessageCounters::new(local_id).save(tx).await?;
            ConversationCounters::new(local_id).save(tx).await?;
        }
    }
    Ok(())
}
