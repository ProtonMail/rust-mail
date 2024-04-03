/// List of Proton colors defined by designers.
static PROTON_COLORS: [&str; 15] = [
    "#0F735A", "#059A6F", "#1ED19C", "#3CBB3A", "#3C8B8C", "#6638B7", "#9553F9", "#9C89FF",
    "#A839A4", "#52006A", "#213474", "#0047AB", "#4989FF", "#29C0E6", "#415DF0",
];

/// Returns hexadecimal Proton color based on string value.
pub fn proton_color(name: &str) -> &str {
    let mut hash = 0;
    for c in name.chars() {
        hash = (c as u32 + ((hash << 5) - hash)) % (65537);
    }
    let index = hash as usize % PROTON_COLORS.len();
    PROTON_COLORS[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proton_color() {
        assert_eq!(proton_color("John Doe"), "#3C8B8C");
        assert_eq!(proton_color("Jane Doe"), "#0F735A");
        assert_eq!(proton_color("Test"), "#A839A4");
        assert_eq!(proton_color(""), "#0F735A");
    }
}
