#[cfg(test)]
#[path = "tests/avatar.rs"]
mod tests;

use crate::datatypes::MessageAddress;
use crate::proton_color::proton_color;

#[derive(Debug, Clone, Eq, PartialEq)]
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
    let mut s = String::with_capacity(1);

    for c in name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
    {
        s.push(c);

        if !s.is_empty() {
            break;
        }
    }

    s
}

fn email_text(address: &str) -> String {
    let local = match address.trim_matches(['<', '>']).split('@').next() {
        Some(first) => first.trim(),
        None => return "?".to_owned(),
    };

    let mut chars = local.chars();

    if let Some(first) = chars.next() {
        return format!("{}", first.to_uppercase());
    }

    "?".to_owned()
}
