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
#[test_case("🙂" => ""; "emoji")]
#[test_case("🙂 John" => "J"; "John with emoji")]
#[test_case("🙂 John Doe" => "J")]
#[test_case("brains@tracyisland.com" => "B")]
#[test_case("    brains@tracyisland.com" => "B"; "leading spaces")]
#[test_case("A@test.com" => "A")]
#[test_case("<brains@tracyisland.com>" => "B"; "brackets")]
#[test_case("@nolocal.com" => "N")]
#[test_case("Riri Fifi Loulou" => "R")]
#[test_case("emojiname@test.com`" => "E")]
#[test_case("OnePart" => "O")]
#[test_case("onepart@test.com" => "O")]
#[test_case("🧑‍🔬 Doctor Rebecca" => "D")]
fn test_avatar_text(name: &str) -> String {
    AvatarInformation::from(name).text
}
