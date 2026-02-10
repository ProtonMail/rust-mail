use crate::parameters::language::Language;
use crate::parameters::language::is_language_param;

#[test]
fn language_param() {
    assert!(is_language_param(&["zh-cmn-Hans-CN".to_owned()]));
    assert!(!is_language_param(&[]));
    assert!(!is_language_param(&[
        "zh-cmn-Hans-CN".to_owned(),
        "zh-cmn-Hans-CN".to_owned()
    ]));
}
