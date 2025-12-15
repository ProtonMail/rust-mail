use crate::MailContextError;
use proton_core_common::{
    datatypes::LocalLabelId,
    models::{Label, ModelExtension},
};
use proton_mail_api::services::proton::prelude::RunningTasks;
use stash::stash::Tether;
use std::ops::ControlFlow;
use tracing::{info, instrument};

#[instrument(skip_all, fields(id))]
pub async fn ensure_label_is_idle(
    tether: &mut Tether,
    id: LocalLabelId,
    tasks: &RunningTasks,
) -> Result<ControlFlow<()>, MailContextError> {
    if let Some(label) = Label::find_by_id(id, tether).await?
        && label.is_busy(tether).await?
    {
        if tasks.is_none() {
            tether.tx(async |bond| label.mark_idle(bond).await).await?;

            Ok(ControlFlow::Continue(()))
        } else {
            info!("Label is busy, pretending the label is empty");

            Ok(ControlFlow::Break(()))
        }
    } else {
        Ok(ControlFlow::Continue(()))
    }
}
