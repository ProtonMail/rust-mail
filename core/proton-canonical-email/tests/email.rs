use proton_canonical_email::{CanonicalEmail, CanonicalizeScheme};

fn test_canonicalize(email: &str, scheme: CanonicalizeScheme, expected: &str) {
    let result = CanonicalEmail::with_scheme(email, scheme);
    assert_eq!(result.as_str(), expected, "Failed on email: {email}");
}

fn test_canonicalize_auto(email: &str, expected: &str) {
    let result = CanonicalEmail::from(email);
    assert_eq!(result.as_str(), expected, "Failed on email: {email}");
}

#[test]
fn test_protonmail_scheme() {
    let email = "Test.Address-1_2+group@protonmail.com";
    let expected = "testaddress12@protonmail.com";
    test_canonicalize(email, CanonicalizeScheme::Proton, expected);
    test_canonicalize_auto(email, expected);
}

#[test]
fn test_gmail_scheme() {
    let email = "Test.Address-1_2+group@gmail.com";
    let expected = "testaddress-1_2@gmail.com";
    test_canonicalize(email, CanonicalizeScheme::Gmail, expected);
    test_canonicalize_auto(email, expected);
}

#[test]
fn test_plus_scheme() {
    let email = "Test.Address-1_2+group@outlook.com";
    let expected = "test.address-1_2@outlook.com";
    test_canonicalize(email, CanonicalizeScheme::Plus, expected);
    test_canonicalize_auto(email, expected);
}

#[test]
fn test_absence_of_scheme() {
    let email = "Test.Address-1_2+group@yahoo.com";
    let expected = "test.address-1_2+group@yahoo.com";
    test_canonicalize(email, CanonicalizeScheme::Default, expected);
    test_canonicalize_auto(email, expected);
}

#[test]
fn test_protonmail_scheme_edge_cases() {
    // Case: Email with only special characters in local part
    let email = ".-_.+group@protonmail.com";
    let expected = "@protonmail.com"; // Special characters stripped, only `@` remains
    test_canonicalize(email, CanonicalizeScheme::Proton, expected);
    test_canonicalize_auto(email, expected);

    // Case: Local part ends with '+'
    let email = "user+@protonmail.com";
    let expected = "user@protonmail.com"; // `+` and everything after is ignored
    test_canonicalize(email, CanonicalizeScheme::Proton, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email with capital letters and special characters
    let email = "Test.Address-1_2@protonmail.ch";
    let expected = "testaddress12@protonmail.ch"; // Lowercased and special characters stripped
    test_canonicalize(email, CanonicalizeScheme::Proton, expected);
    test_canonicalize_auto(email, expected);
}

#[test]
fn test_gmail_scheme_edge_cases() {
    // Case: Email with multiple '.' and special characters
    let email = "Te..st..Address-1_2+info@gmail.com";
    let expected = "testaddress-1_2@gmail.com"; // All `.` stripped, and everything after `+` is ignored
    test_canonicalize(email, CanonicalizeScheme::Gmail, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email with no local part before '+'
    let email = "+tag@gmail.com";
    let expected = "@gmail.com"; // Everything before `@` is ignored due to `+`
    test_canonicalize(email, CanonicalizeScheme::Gmail, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email with mixed case and special characters
    let email = "Test...Address+Extra@gmail.com";
    let expected = "testaddress@gmail.com"; // All `.` stripped, and everything after `+` is ignored
    test_canonicalize(email, CanonicalizeScheme::Gmail, expected);
    test_canonicalize_auto(email, expected);
}

#[test]
fn test_plus_scheme_edge_cases() {
    // Case: Email with multiple '+' characters
    let email = "user+group+info@outlook.com";
    let expected = "user@outlook.com"; // Everything after the first `+` is ignored
    test_canonicalize(email, CanonicalizeScheme::Plus, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email with special characters and '+' in local part
    let email = "Test-Address_+info+extra@mail.ru";
    let expected = "test-address_@mail.ru"; // Lowercase and ignore characters after first `+`
    test_canonicalize(email, CanonicalizeScheme::Plus, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email with only '+' in the local part
    let email = "+@yandex.ru";
    let expected = "@yandex.ru"; // Everything before `@` is ignored due to `+`
    test_canonicalize(email, CanonicalizeScheme::Plus, expected);
    test_canonicalize_auto(email, expected);
}

#[test]
fn test_absence_of_scheme_edge_cases() {
    // Case: Email with capital letters and special characters
    let email = "Test.Address-1_2+group@yahoo.com";
    let expected = "test.address-1_2+group@yahoo.com"; // Only lowercased, rest unchanged
    test_canonicalize(email, CanonicalizeScheme::Default, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email with special characters and custom domain
    let email = "User.Name+Info@custom-domain.org";
    let expected = "user.name+info@custom-domain.org"; // Only lowercased, rest unchanged
    test_canonicalize(email, CanonicalizeScheme::Default, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email with numeric and special characters
    let email = "User.123-456+group@tutanota.com";
    let expected = "user.123-456+group@tutanota.com"; // Only lowercased, rest unchanged
    test_canonicalize(email, CanonicalizeScheme::Default, expected);
    test_canonicalize_auto(email, expected);
}

#[test]
fn test_empty_and_invalid_inputs() {
    // Case: Empty email string
    let email = "";
    let expected = "";
    test_canonicalize(email, CanonicalizeScheme::Default, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email without domain part
    let email = "user+info";
    let expected = "user+info";
    test_canonicalize(email, CanonicalizeScheme::Default, expected);
    test_canonicalize_auto(email, expected);

    // Case: Email with only '@' character
    let email = "@";
    let expected = "@";
    test_canonicalize(email, CanonicalizeScheme::Default, expected);
    test_canonicalize_auto(email, expected);
}
