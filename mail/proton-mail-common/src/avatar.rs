use crate::proton_color::proton_color;
use proton_api_mail::domain::MessageAddress;

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AvatarInformation {
    pub text: String,
    pub color: String,
}

/// Contains the details used for the avatar shown for a conversation.
///
/// It contains:
///     - the text to display in the avatar,
///     - the color to use for the avatar,
impl AvatarInformation {
    /// Takes display name and email address and uses these to determine the text and color the avatar should be.
    pub fn build(display_name: &str, email: &str) -> AvatarInformation {
        AvatarInformation {
            text: avatar_text(display_name, email),
            color: proton_color(display_name).to_string(),
        }
    }

    /// Creates an AvatarInformation struct using the details of the first MessageAddress in the provided slice.
    pub fn from_message_addresses(address_list: &[MessageAddress]) -> AvatarInformation {
        let first_sender = address_list.first();
        let display_name_email = match first_sender {
            Some(first_sender) => (first_sender.name.as_str(), first_sender.address.as_str()),
            None => ("", ""),
        };

        AvatarInformation::build(display_name_email.0, display_name_email.1)
    }

    /// Creates an AvatarInformation struct using a MessageAddress.
    pub fn from_message_address(address: &MessageAddress) -> AvatarInformation {
        AvatarInformation::build(address.name.as_str(), address.address.as_str())
    }
}

pub fn avatar_text(name: &str, email: &str) -> String {
    let initials = initials(name);
    if !initials.is_empty() {
        let mut chars = initials.chars();
        if let Some(first) = chars.next() {
            match chars.next() {
                Some(second) => {
                    return format!("{}{}", first.to_uppercase(), second.to_lowercase())
                }
                None => return format!("{}", first.to_uppercase()),
            }
        }
    }

    email_text(email)
}

fn initials(name: &str) -> String {
    let mut s = String::with_capacity(2);

    for c in name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
    {
        s.push(c);

        if s.len() == 2 {
            break;
        }
    }

    s
}

fn email_text(address: &str) -> String {
    let local = match address.trim_matches(&['<', '>']).split('@').next() {
        Some(first) => first.trim(),
        None => return "??".to_string(),
    };

    let mut chars = local.chars();

    if let Some(first) = chars.next() {
        match chars.next() {
            Some(second) => return format!("{}{}", first.to_uppercase(), second),
            None => return format!("{}", first.to_uppercase()),
        }
    }

    "??".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initials() {
        assert_eq!(initials("John Doe"), "JD");
        assert_eq!(initials("john doe"), "jd");
        assert_eq!(initials("John"), "J");
        assert_eq!(initials(""), "");
        assert_eq!(initials("J"), "J");
        assert_eq!(initials("John 1Doe"), "J1");
        assert_eq!(initials("123 John"), "1J");
        assert_eq!(initials("🙂 John Doe"), "JD");
    }

    #[test]
    fn test_email_text() {
        assert_eq!(email_text("brains@tracyisland.com"), "Br");
        assert_eq!(email_text("    brains@tracyisland.com"), "Br");
        assert_eq!(email_text("A@test.com"), "A");
        assert_eq!(email_text("<brains@tracyisland.com>"), "Br");
        assert_eq!(email_text("@nolocal.com"), "??");
    }

    #[test]
    fn test_avatar_text() {
        assert_eq!(avatar_text("Riri Fifi Loulou", "rifilou@test.com"), "Rf");
        assert_eq!(avatar_text("🙂", "emojiname@test.com`"), "Em");
        assert_eq!(avatar_text("OnePart", "onepart@test.com"), "O")
    }
}
