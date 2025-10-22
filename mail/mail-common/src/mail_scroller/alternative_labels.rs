use crate::{MailContextError, datatypes::SystemLabelId};
use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::{LocalLabelId, SystemLabel},
    models::{Label, ModelIdExtension},
};
use stash::stash::Tether;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlternativeLabels {
    pub label: LocalLabelId,
    pub alt_label: Option<LocalLabelId>,
}

impl AlternativeLabels {
    pub async fn new(label: LocalLabelId, tether: &Tether) -> Result<Self, MailContextError> {
        let alt_label = Self::alternative_local_label(label, tether).await?;

        Ok(Self { label, alt_label })
    }

    pub fn supports_include_filter(&self) -> bool {
        self.alt_label.is_some()
    }

    /// If `id` points at the `All Mail` label, this function returns id of the
    /// `Almost All Mail` label; otherwise it returns `None`.
    async fn alternative_local_label(
        id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Option<LocalLabelId>, MailContextError> {
        let Some(id) = Label::local_id_counterpart(id, tether).await? else {
            return Ok(None);
        };

        if id == LabelId::almost_all_mail() {
            Ok(SystemLabel::AllMail.local_id(tether).await?)
        } else {
            Ok(None)
        }
    }
}
