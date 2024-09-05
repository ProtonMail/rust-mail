use crate::UniffiEnum;
use proton_mail_common::actions::GeneralActions as RealGeneralActions;

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
