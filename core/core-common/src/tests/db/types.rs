#![allow(non_snake_case)]

use crate::datatypes::{AccountDetails, AvatarInformation};
use crate::db::account::CoreAccount;
use proton_api_core::services::proton::common::UserId;

#[cfg(test)]
mod core_account_account_details_tests {
    use super::*;

    #[test]
    fn test_all_fields_present() {
        let sut = test_account(TestCoreAccountParams {
            name_or_addr: "frank.moon@pm.me".to_string(),
            display_name: Some("Frankie".to_string()),
            username: Some("Frank Moon".to_string()),
            primary_addr: Some("frank@proton.me".to_string()),
        });

        let result = sut.account_details();

        assert_account_details(result, "Frankie", "frank@proton.me");
    }

    #[test]
    fn test_no_display_name_fallback_to_username() {
        let sut = test_account(TestCoreAccountParams {
            name_or_addr: "Max Johnson".to_string(),
            display_name: None,
            username: Some("Max".to_string()),
            primary_addr: Some("max@pm.me".to_string()),
        });

        let result = sut.account_details();

        assert_account_details(result, "Max", "max@pm.me");
    }

    #[test]
    fn test_no_display_name_or_username_fallback_to_name_or_addr() {
        let sut = test_account(TestCoreAccountParams {
            name_or_addr: "John Doe".to_string(),
            display_name: None,
            username: None,
            primary_addr: Some("john@gmail.com".to_string()),
        });

        let result = sut.account_details();

        assert_account_details(result, "John Doe", "john@gmail.com");
    }

    #[test]
    fn test_no_primary_addr_fallback_to_name_or_addr() {
        let sut = test_account(TestCoreAccountParams {
            name_or_addr: "dricus@proton.me".to_string(),
            display_name: Some("Dricus".to_string()),
            username: Some("Dricus Du Plessis".to_string()),
            primary_addr: None,
        });

        let result = sut.account_details();

        assert_account_details(result, "Dricus", "dricus@proton.me");
    }

    fn assert_account_details(result: AccountDetails, expected_name: &str, expected_email: &str) {
        assert_eq!(result.name, expected_name);
        assert_eq!(result.email, expected_email);
        assert_eq!(
            result.avatar_information,
            AvatarInformation::from(expected_name).into()
        );
    }
}

struct TestCoreAccountParams {
    name_or_addr: String,
    display_name: Option<String>,
    username: Option<String>,
    primary_addr: Option<String>,
}

fn test_account(params: TestCoreAccountParams) -> CoreAccount {
    CoreAccount {
        remote_id: UserId::new("__NOT_USED__".to_string()),
        name_or_addr: params.name_or_addr,
        display_name: params.display_name,
        username: params.username,
        primary_addr: params.primary_addr,
        second_factor_mode: None,
        password_mode: None,
        primary_at: None,
        is_ready: false,
        row_id: None,
    }
}
