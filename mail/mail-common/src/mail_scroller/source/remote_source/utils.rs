use crate::MailContextError;
use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::LocalLabelId,
    models::{Label, ModelExtension},
};
use proton_mail_api::services::proton::prelude::RunningTasks;
use stash::stash::Tether;
use std::ops::ControlFlow;
use tracing::{info, instrument};

#[instrument(skip_all, fields(?local_id, ?remote_id))]
pub async fn ensure_label_is_idle(
    tether: &mut Tether,
    local_id: LocalLabelId,
    remote_id: &LabelId,
    tasks: &RunningTasks,
) -> Result<ControlFlow<()>, MailContextError> {
    if let Some(label) = Label::find_by_id(local_id, tether).await?
        && label.is_busy(tether).await?
    {
        if tasks.has(remote_id) {
            info!("Label is busy, pretending the response was empty");

            Ok(ControlFlow::Break(()))
        } else {
            tether.tx(async |bond| label.mark_idle(bond).await).await?;

            Ok(ControlFlow::Continue(()))
        }
    } else {
        Ok(ControlFlow::Continue(()))
    }
}
