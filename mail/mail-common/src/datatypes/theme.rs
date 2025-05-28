/// Information about color scheme used in the UI by the application.
/// It affects on which CSS style is used in the HTML body of the message
///
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MailTheme {
    LightMode,
    DarkMode,
}
