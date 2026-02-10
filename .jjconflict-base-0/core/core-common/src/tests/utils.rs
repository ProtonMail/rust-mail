use super::*;
use test_case::test_case;

#[test_case("a" => "A")]
#[test_case("B" => "B")]
#[test_case("1" => "1")]
#[test_case("y̆es" => "Y̆")]
#[test_case("@user" => "@")]
#[test_case("🗻∈🌏" => "🗻")]
#[test_case("\"This is a quote\"" => "\"")]
#[test_case("🧑‍🔬 Doctor Rebecca" => "🧑‍🔬")]
fn test_first_grapheme_uppercase(s: &str) -> String {
    first_grapheme_upppercase(s).unwrap_or_default()
}

#[test]
fn test_proton_color() {
    assert_eq!(proton_color("John Doe"), "#3F8B8E");
    assert_eq!(proton_color("Jane Doe"), "#2E8378");
    assert_eq!(proton_color("Test"), "#A1439F");
    assert_eq!(proton_color(""), "#2E8378");
}
