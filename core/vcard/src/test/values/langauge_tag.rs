use crate::values::language_tag::{LanguageTag, is_language_tag_value};
use oxilangtag::LanguageTag as OxiLanguageTag;

#[test]
fn language_tag_struct() {
    let language_tag = LanguageTag::new_validated("zh-cmn-Hans-CN").unwrap();
    assert_eq!(
        language_tag.0,
        OxiLanguageTag::parse("zh-cmn-Hans-CN").unwrap()
    );
}

#[test]
fn language_tag_value() {
    assert!(is_language_tag_value("zh-cmn-Hans-CN"));
}
