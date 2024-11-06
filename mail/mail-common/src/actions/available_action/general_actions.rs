/// General actions that can be performed on a message.
/// These actions are a hardcoded options to show on the edit panel.
/// It was agreed that they will be unified on the rust side.
///
#[derive(Debug, Clone, PartialEq)]
pub enum GeneralActions {
    Print,
    ReportPhishing,
    SaveAsPdf,
    ViewHeaders,
    ViewHtml,
    ViewMessageInDarkMode,
    ViewMessageInLightMode,
}

impl GeneralActions {
    pub fn all() -> Vec<Self> {
        vec![
            GeneralActions::ViewMessageInLightMode,
            GeneralActions::ViewMessageInDarkMode,
            GeneralActions::SaveAsPdf,
            GeneralActions::Print,
            GeneralActions::ViewHeaders,
            GeneralActions::ViewHtml,
            GeneralActions::ReportPhishing,
        ]
    }
}
