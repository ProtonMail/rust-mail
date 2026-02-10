/// Check whether the given `address` is a valid email address.
#[uniffi::export]
#[must_use]
pub fn is_valid_email_address(address: &str) -> bool {
    proton_core_common::validation::is_valid_email_address(address)
}
