#![allow(non_snake_case)]

use super::AvatarInformation;
use test_case::test_case;

#[test_case("John Doe" => "J"; "John Doe uppercase")]
#[test_case("john doe" => "J"; "John Doe lowercase")]
#[test_case("John" => "J")]
#[test_case("" => ""; "empty")]
#[test_case("J" => "J")]
#[test_case("John 1Doe" => "J")]
#[test_case("123 John" => "1")]
#[test_case("🙂" => "🙂"; "emoji")]
#[test_case("🙂 John" => "🙂"; "John with emoji")]
#[test_case("🙂 John Doe" => "🙂")]
#[test_case("brains@tracyisland.com" => "B")]
#[test_case("    brains@tracyisland.com" => "B"; "leading spaces")]
#[test_case("A@test.com" => "A")]
#[test_case("<brains@tracyisland.com>" => "B"; "brackets")]
#[test_case("@nolocal.com" => "N")]
#[test_case("Riri Fifi Loulou" => "R")]
#[test_case("emojiname@test.com`" => "E")]
#[test_case("OnePart" => "O")]
#[test_case("onepart@test.com" => "O")]
#[test_case("🧑‍🔬 Doctor Rebecca" => "🧑‍🔬")]
#[test_case("Milti-Part Surname" => "M")] // Name with dashes
#[test_case("日本人の氏名" => "日")] // Japanese
#[test_case("ім'я прізвище" => "І")] // Ukrainian (Cyrillic)
#[test_case("שם משפחה" => "ש")] // Hebrew
fn test_avatar_text(name: &str) -> String {
    AvatarInformation::from(name).text
}
