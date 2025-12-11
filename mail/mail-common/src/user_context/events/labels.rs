use crate::AppError;
use crate::models::{ConversationCounters, MessageCounters};
use proton_core_api::services::proton::LabelId;
use proton_core_common::event_loop::events::{Action, LabelEvent};
use proton_core_common::models::{Label, ModelIdExtension};
use stash::orm::Model;
use stash::stash::Bond;

pub async fn handle_counters_label_events(
    tx: &Bond<'_>,
    label_events: &[LabelEvent],
) -> Result<(), AppError> {
    for label_event in label_events {
        handle_counters_label_event(tx, &label_event.remote_id, label_event.action).await?;
    }
    Ok(())
}

pub async fn handle_counters_label_event(
    tx: &Bond<'_>,
    id: &LabelId,
    action: Action,
) -> Result<(), AppError> {
    if action == Action::Create {
        tracing::info!("Creating message and conversation counters for {id:?}",);
        let local_id = Label::remote_id_counterpart(id.clone(), tx)
            .await?
            .ok_or_else(|| AppError::RemoteLabelDoesNotExist(id.clone()))?;
        MessageCounters::new(local_id).save(tx).await?;
        ConversationCounters::new(local_id).save(tx).await?;
    }
    Ok(())
}
