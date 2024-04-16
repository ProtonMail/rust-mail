/// Result of [`locate_blockquote`], `before` contains the parent message and `after` contains
/// the remainder.
#[derive(uniffi::Record)]
pub struct LocateBlockquoteResult {
    pub before: String,
    pub after: String,
}

///Try to locate the eventual blockquote present in the document no matter the expeditor of the mail
///
///Return the HTML content split at the blockquote start
#[uniffi::export]
#[must_use]
pub fn locate_blockquote(input: &str) -> LocateBlockquoteResult {
    let (before, after) = proton_mail_message_detector::locate_blockquote(input);
    LocateBlockquoteResult { before, after }
}
