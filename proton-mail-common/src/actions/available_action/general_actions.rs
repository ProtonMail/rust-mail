#[derive(Debug, Clone, PartialEq)]
pub enum GeneralActions {
    ViewMessageInLightMode,
    SaveAsPdf,
    Print,
    ViewHeaders,
    ViewHtml,
    ReportPhishing,
}

impl GeneralActions {
    pub fn all() -> Vec<Self> {
        vec![
            GeneralActions::ViewMessageInLightMode,
            GeneralActions::SaveAsPdf,
            GeneralActions::Print,
            GeneralActions::ViewHeaders,
            GeneralActions::ViewHtml,
            GeneralActions::ReportPhishing,
        ]
    }
}
