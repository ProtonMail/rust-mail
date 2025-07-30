use serde::{self, Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::OnceLock;

macro_rules! create_domain_set {
    ($fn_name:ident, $const_name:ident) => {
        fn $fn_name() -> &'static HashSet<&'static str> {
            static HASHSET: OnceLock<HashSet<&'static str>> = OnceLock::new();

            HASHSET.get_or_init(|| $const_name.iter().copied().collect())
        }
    };
}

const PROTONMAIL_DOMAINS: &[&str] = &[
    "protonmail.com",
    "protonmail.ch",
    "pm.me",
    "proton.me",
    "proton.ch",
    "external.proton.ch",
];

const GMAIL_DOMAINS: &[&str] = &["gmail.com", "googlemail.com", "google.com"];

const PLUS_DOMAINS: &[&str] = &[
    "hotmail.com",
    "hotmail.co.uk",
    "hotmail.fr",
    "outlook.com",
    "yandex.ru",
    "mail.ru",
];

create_domain_set!(protonmail_domains, PROTONMAIL_DOMAINS);
create_domain_set!(gmail_domains, GMAIL_DOMAINS);
create_domain_set!(plus_domains, PLUS_DOMAINS);

/// Formats an email address to it's canonical form.
///
/// Many email service providers internally convert email addresses to a standardized, canonical form.
/// This process ensures that variations of an email address are treated as equivalent, directing emails to the same inbox.
/// This function canonicalizes an email address according to one of the `Proton` schemes.
/// `Proton` uses the following four canonicalization schemes:
///
/// ### `Proton` Scheme:
/// - **Actions**: Convert to lowercase, ignore characters starting from `+`, and strip all `.`, `-`, and `_`.
/// - **Example**: `Test.Address-1_2+group@protonmail.com` → `testaddress12@protonmail.com`
/// - **Providers**: `protonmail.com`, `protonmail.ch`, `pm.me`
///
/// ### Gmail Scheme:
/// - **Actions**: Convert to lowercase, ignore characters starting from `+`, and strip all `.`.
/// - **Example**: `Test.Address-1_2+group@protonmail.com` → `testaddress-1_2@protonmail.com`
/// - **Providers**: `gmail.com`
///
/// ### Plus Scheme:
/// - **Actions**: Convert to lowercase and ignore characters starting from `+`.
/// - **Example**: `Test.Address-1_2+group@protonmail.com` → `test.address-1_2@protonmail.com`
/// - **Providers**: `hotmail.com`, `hotmail.fr`, `hotmail.co.uk`, `outlook.com`, `yandex.ru`, `mail.ru`
///
/// ### Absence of Scheme:
/// - **Actions**: Convert to lowercase only.
/// - **Example**: `Test.Address-1_2+group@protonmail.com` → `test.address-1_2+group@protonmail.com`
/// - **Providers**: `DEFAULT`, custom domains, `yahoo.com`, `yahoo.fr`, `yahoo.co.uk`, `aol.com`, `tutanota.com`, `gmx.de`
///
/// See [Proton confluence](https://confluence.protontech.ch/display/MBE/Canonize+email+addresses) for more details.
#[must_use]
pub fn canonicalize(email: &str, scheme: CanonicalizeScheme) -> CanonicalEmail {
    if matches!(scheme, CanonicalizeScheme::Default) {
        return CanonicalEmail(email.to_lowercase());
    }
    let (local_part, at, domain_part) = email_parts(email);
    let normalized_local_part = scheme.normalize_local_part(local_part);
    let normalized_domain = CanonicalizeScheme::normalize_domain(domain_part);
    CanonicalEmail(format!("{normalized_local_part}{at}{normalized_domain}"))
}

/// Formats an email address to it's canonical form by guessing the [`CanonicalizeScheme`]
/// from its domain.
///
/// See [`canonicalize`] for more details why and how the canonical form is determined.
#[must_use]
pub fn canonicalize_auto(email: &str) -> CanonicalEmail {
    let (_, _, domain_part) = email_parts(email);
    let scheme = CanonicalizeScheme::infer_from_domain(domain_part);
    canonicalize(email, scheme)
}

/// Type for the email canonicalize scheme to be used.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum CanonicalizeScheme {
    Default,
    Plus,
    Gmail,
    Proton,
}

impl CanonicalizeScheme {
    /// Helper function to determine the `CanonicalizeScheme` from the domain.
    fn infer_from_domain(domain: &str) -> Self {
        match CanonicalizeScheme::normalize_domain(domain).as_str() {
            domain if protonmail_domains().contains(domain) => CanonicalizeScheme::Proton,
            domain if gmail_domains().contains(domain) => CanonicalizeScheme::Gmail,
            domain if plus_domains().contains(domain) => CanonicalizeScheme::Plus,
            _ => CanonicalizeScheme::Default,
        }
    }

    /// Helper function to normalize the local part of an email address with the scheme.
    fn normalize_local_part(self, local_part: &str) -> String {
        match self {
            CanonicalizeScheme::Default => local_part.to_lowercase(),
            CanonicalizeScheme::Plus => remove_plus_alias_local_part(local_part).to_lowercase(),
            CanonicalizeScheme::Gmail => remove_plus_alias_local_part(local_part)
                .chars()
                .filter(|&c| c != '.')
                .collect::<String>()
                .to_lowercase(),
            CanonicalizeScheme::Proton => remove_plus_alias_local_part(local_part)
                .chars()
                .filter(|&c| c != '.' && c != '_' && c != '-')
                .collect::<String>()
                .to_lowercase(),
        }
    }

    /// Helper function to normalize the domain part of an email address.
    fn normalize_domain(domain: &str) -> String {
        domain.to_lowercase()
    }
}

/// `CanonicalEmail` represents an email address that has been transformed into its canonical form.
/// This ensures that variations of the email address are treated as equivalent.
///
/// Email providers often apply canonicalization rules to email addresses to simplify the management of email variations.
/// For example, different email addresses with slight variations might end up pointing to the same inbox.
/// The `CanonicalEmail` type encapsulates an email address that has been canonicalized according to these rules.
///
/// The canonical form of an email can vary depending on the service provider's rules.
/// Proton uses the following rules:
///
/// - **Proton Scheme**:
///   - Actions: Convert to lowercase, ignore characters after a `+`, and remove all periods (`.`), hyphens (`-`), and underscores (`_`).
///   - Example: `Test.Address-1_2+group@protonmail.com` becomes `testaddress12@protonmail.com`.
///   - Providers: `protonmail.com`, `protonmail.ch`, `pm.me`, `proton.me`.
///
/// - **Gmail Scheme**:
///   - Actions: Convert to lowercase, ignore characters after a `+`, and remove all periods (`.`).
///   - Example: `Test.Address-1_2+group@gmail.com` becomes `testaddress-1_2@gmail.com`.
///   - Providers: `gmail.com`.
///
/// - **Plus Scheme**:
///   - Actions: Convert to lowercase and ignore characters after a `+`.
///   - Example: `Test.Address-1_2+group@hotmail.com` becomes `test.address-1_2@hotmail.com`.
///   - Providers: `hotmail.com`, `hotmail.co.uk`, `hotmail.fr`, `outlook.com`, `yandex.ru`, `mail.ru`.
///
/// - **Default Scheme**:
///   - Actions: Convert to lowercase without additional modifications.
///   - Example: `Test.Address-1_2+group@custom-domain.com` becomes `test.address-1_2+group@custom-domain.com`.
///   - Providers: Custom domains and others like `yahoo.com`, `yahoo.fr`, `yahoo.co.uk`, `aol.com`, `tutanota.com`, `gmx.de`.
///
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Hash, Clone)]
#[serde(crate = "self::serde")]
pub struct CanonicalEmail(String);

impl CanonicalEmail {
    /// Transforms the input to a canonical email with [`canonicalize`].
    pub fn with_scheme<T: AsRef<str>>(value: T, scheme: CanonicalizeScheme) -> Self {
        canonicalize(value.as_ref(), scheme)
    }

    /// Returns a read-only view of the canonical email address as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: AsRef<str>> From<T> for CanonicalEmail {
    fn from(value: T) -> Self {
        canonicalize_auto(value.as_ref())
    }
}

impl Display for CanonicalEmail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Helper function that splits the email into (local part, at i.e.(`@`), domain part)
fn email_parts(email: &str) -> (&str, &str, &str) {
    let Some(split_index) = email.rfind('@') else {
        return (email, "", "");
    };
    (&email[..split_index], "@", &email[(split_index + 1)..])
}

/// Helper function to remove the content after the `+` in the local part.
fn remove_plus_alias_local_part(local_part: &str) -> &str {
    local_part.split('+').next().unwrap_or_default()
}
