use crate::datatypes::{AccountDetails, AvatarInformation};
use crate::db::account::CoreAccount;
use proton_core_api::services::proton::UserId;
use std::default::Default;

impl Default for CoreAccount {
    fn default() -> Self {
        Self::new(UserId::new("__NOT_USED__".to_string()), String::new())
    }
}

#[cfg(test)]
mod core_account_details_tests {
    use super::*;

    #[test]
    fn test_all_fields_present() {
        let sut = CoreAccount {
            name_or_addr: "frank.moon@pm.me".to_string(),
            display_name: Some("Frankie".to_string()),
            username: Some("Frank Moon".to_string()),
            primary_addr: Some("frank@proton.me".to_string()),
            ..Default::default()
        };

        let result = sut.details();

        assert_account_details(&result, "Frankie", "frank@proton.me");
    }

    #[test]
    fn test_no_display_name_fallback_to_username() {
        let sut = CoreAccount {
            name_or_addr: "Max Johnson".to_string(),
            display_name: None,
            username: Some("Max".to_string()),
            primary_addr: Some("max@pm.me".to_string()),
            ..Default::default()
        };

        let result = sut.details();

        assert_account_details(&result, "Max", "max@pm.me");
    }

    #[test]
    fn test_no_display_name_or_username_fallback_to_name_or_addr() {
        let sut = CoreAccount {
            name_or_addr: "John Doe".to_string(),
            display_name: None,
            username: None,
            primary_addr: Some("john@gmail.com".to_string()),
            ..Default::default()
        };

        let result = sut.details();

        assert_account_details(&result, "John Doe", "john@gmail.com");
    }

    #[test]
    fn test_no_primary_addr_fallback_to_name_or_addr() {
        let sut = CoreAccount {
            name_or_addr: "dricus@proton.me".to_string(),
            display_name: Some("Dricus".to_string()),
            username: Some("Dricus Du Plessis".to_string()),
            primary_addr: None,
            ..Default::default()
        };

        let result = sut.details();

        assert_account_details(&result, "Dricus", "dricus@proton.me");
    }

    #[test]
    fn test_blank_display_name_fallback_to_username() {
        let sut = CoreAccount {
            name_or_addr: "dricus@proton.me".to_string(),
            display_name: Some(String::new()),
            username: Some("Dricus Du Plessis".to_string()),
            primary_addr: None,
            ..Default::default()
        };

        let result = sut.details();

        assert_account_details(&result, "Dricus Du Plessis", "dricus@proton.me");
    }

    #[test]
    fn test_blank_display_name_or_username_fallback_to_name_or_addr() {
        let sut = CoreAccount {
            name_or_addr: "dricus@proton.me".to_string(),
            display_name: Some(String::new()),
            username: Some(String::new()),
            primary_addr: None,
            ..Default::default()
        };

        let result = sut.details();

        assert_account_details(&result, "dricus@proton.me", "dricus@proton.me");
    }

    fn assert_account_details(result: &AccountDetails, expected_name: &str, expected_email: &str) {
        assert_eq!(result.name, expected_name);
        assert_eq!(result.email, expected_email);
        assert_eq!(
            result.avatar_information,
            AvatarInformation::from(expected_name)
        );
    }
}
