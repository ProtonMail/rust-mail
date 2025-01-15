#![allow(non_snake_case)]

use crate::datatypes::{AccountDetails, AvatarInformation};
use crate::db::account::CoreAccount;
use proton_api_core::services::proton::common::UserId;

#[test]
fn test_account_details() {
    let test_cases = vec![
        // All fields present
        (
            test_account(TestAccountParams {
                name_or_addr: "frank.moon@pm.me".to_string(),
                display_name: Some("Frankie".to_string()),
                username: Some("Frank Moon".to_string()),
                primary_addr: Some("frank@proton.me".to_string()),
            }),
            AccountDetails {
                name: "Frankie".to_string(),
                email: "frank@proton.me".to_string(),
                avatar_information: AvatarInformation::from("Frankie").into(),
            },
        ),
        // No display_name, fallback to username
        (
            test_account(TestAccountParams {
                name_or_addr: "Max Johnson".to_string(),
                display_name: None,
                username: Some("Max".to_string()),
                primary_addr: Some("max@pm.me".to_string()),
            }),
            AccountDetails {
                name: "Max".to_string(),
                email: "max@pm.me".to_string(),
                avatar_information: AvatarInformation::from("Max").into(),
            },
        ),
        // No display_name or username, fallback to name_or_addr
        (
            test_account(TestAccountParams {
                name_or_addr: "John Doe".to_string(),
                display_name: None,
                username: None,
                primary_addr: Some("john@gmail.com".to_string()),
            }),
            AccountDetails {
                name: "John Doe".to_string(),
                email: "john@gmail.com".to_string(),
                avatar_information: AvatarInformation::from("John Doe").into(),
            },
        ),
        // No primary_addr, fallback to name_or_addr for email
        (
            test_account(TestAccountParams {
                name_or_addr: "dricus@proton.me".to_string(),
                display_name: Some("Dricus".to_string()),
                username: Some("Dricus Du Plessis".to_string()),
                primary_addr: None,
            }),
            AccountDetails {
                name: "Dricus".to_string(),
                email: "dricus@proton.me".to_string(),
                avatar_information: AvatarInformation::from("Dricus").into(),
            },
        ),
    ];

    for (account, expected) in test_cases {
        let result = account.account_details();
        assert_eq!(result.name, expected.name);
        assert_eq!(result.email, expected.email);
        assert_eq!(result.avatar_information, expected.avatar_information);
    }
}
struct TestAccountParams {
    name_or_addr: String,
    display_name: Option<String>,
    username: Option<String>,
    primary_addr: Option<String>,
}

fn test_account(params: TestAccountParams) -> CoreAccount {
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
