pub fn avatar_text(name: &str, email: &str) -> String {
    let initials = initials(name);
    if !initials.is_empty() {
        let mut chars = initials.chars();
        let first = chars.next().unwrap();
        match chars.next() {
            Some(second) => return format!("{}{}", first, second.to_lowercase()),
            None => return format!("{}", first),
        }
    }

    email_text(email)
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .collect::<String>()
        .to_uppercase()
}

fn email_text(address: &str) -> String {
    let local = address
        .trim_matches(&['<', '>'])
        .split('@')
        .next()
        .expect("email str does not contain email address")
        .trim();

    let mut chars = local.chars();

    let first = chars
        .next()
        .expect("email local part is empty")
        .to_uppercase();
    match chars.next() {
        Some(second) => format!("{}{}", first, second),
        None => format!("{}", first),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initials() {
        assert_eq!(initials("John Doe"), "JD");
        assert_eq!(initials("john doe"), "JD");
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
        assert_eq!(email_text("<brains@tracyisland.com>"), "Br")
    }

    #[test]
    fn test_avatar_text() {
        assert_eq!(avatar_text("Riri Fifi Loulou", "rifilou@test.com"), "Rf");
        assert_eq!(avatar_text("🙂", "emojiname@test.com`"), "Em");
        assert_eq!(avatar_text("OnePart", "onepart@test.com"), "O")
    }
}
