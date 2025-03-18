use email_address::{EmailAddress, Options};

/// Check whether the given `email` is valid.
#[must_use]
pub fn is_valid_email_address(email: &str) -> bool {
    parse_email_address(email).is_ok()
}

/// Parse the given `email` address.
///
/// # Errors
///
/// Returns error if the email address is not valid.
pub fn parse_email_address(email: &str) -> Result<EmailAddress, email_address::Error> {
    EmailAddress::parse_with_options(
        email,
        Options::default()
            .without_display_text()
            .with_required_tld(),
    )
}
