pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .collect::<String>()
        .to_uppercase()
}

pub fn avatar_initials(name: &str) -> String {
    let initials = initials(name);
    if initials.is_empty() {
        return String::from("?");
    }
    let mut chars = initials.chars();
    let first = chars.next().unwrap();
    let second = chars.next().unwrap_or(first).to_lowercase().next().unwrap();
    format!("{}{}", first, second)
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

    fn test_avatar_initials() {
        assert_eq!(avatar_initials("Riri Fifi Loulou"), "Rf");
        assert_eq!(avatar_initials("🙂"), "?");
    }
}
