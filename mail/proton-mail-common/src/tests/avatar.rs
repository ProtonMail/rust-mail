#![allow(non_snake_case)]

use super::*;

#[test]
fn test_initials() {
    assert_eq!(initials("John Doe"), "J");
    assert_eq!(initials("john doe"), "j");
    assert_eq!(initials("John"), "J");
    assert_eq!(initials(""), "");
    assert_eq!(initials("J"), "J");
    assert_eq!(initials("John 1Doe"), "J");
    assert_eq!(initials("123 John"), "1");
    assert_eq!(initials("🙂 John Doe"), "J");
}

#[test]
fn test_email_text() {
    assert_eq!(email_text("brains@tracyisland.com"), "B");
    assert_eq!(email_text("    brains@tracyisland.com"), "B");
    assert_eq!(email_text("A@test.com"), "A");
    assert_eq!(email_text("<brains@tracyisland.com>"), "B");
    assert_eq!(email_text("@nolocal.com"), "?");
}

#[test]
fn test_avatar_text() {
    assert_eq!(avatar_text("Riri Fifi Loulou", "rifilou@test.com"), "R");
    assert_eq!(avatar_text("🙂", "emojiname@test.com`"), "E");
    assert_eq!(avatar_text("OnePart", "onepart@test.com"), "O");
    assert_eq!(avatar_text("John Smith", "john@smith.com"), "J");
    assert_eq!(avatar_text("John", "john@smith.com"), "J");
    assert_eq!(avatar_text("", "john@smith.com"), "J");
    assert_eq!(avatar_text("🙂 John", "emojijohn@test.com`"), "J");
}
