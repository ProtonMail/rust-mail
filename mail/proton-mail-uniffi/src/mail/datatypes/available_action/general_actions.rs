use crate::UniffiEnum;
use proton_mail_common::actions::GeneralActions as RealGeneralActions;

/// General actions that can be performed on a message.
/// These actions are a hardcoded options to show on the edit panel.
/// It was agreed that they will be unified on the rust side.
///
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum GeneralActions {
    ViewMessageInLightMode,
    SaveAsPdf,
    Print,
    ViewHeaders,
    ViewHtml,
    ReportPhishing,
}

impl From<RealGeneralActions> for GeneralActions {
    fn from(value: RealGeneralActions) -> Self {
        match value {
            RealGeneralActions::ViewMessageInLightMode => GeneralActions::ViewMessageInLightMode,
            RealGeneralActions::SaveAsPdf => GeneralActions::SaveAsPdf,
            RealGeneralActions::Print => GeneralActions::Print,
            RealGeneralActions::ViewHeaders => GeneralActions::ViewHeaders,
            RealGeneralActions::ViewHtml => GeneralActions::ViewHtml,
            RealGeneralActions::ReportPhishing => GeneralActions::ReportPhishing,
        }
    }
}
