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
            .without_domain_literal()
            .without_display_text()
            .with_long_local_parts()
            .with_required_tld(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_very_long_email_address() {
        let email = "reply+2a907e&3uofr1&&99cd5c22c2ca5b23655799316a8d8eb2dd83c3c487612cb9b9a00bf13f13afe2@mg1.substack.com";
        assert!(is_valid_email_address(email));
    }
}
