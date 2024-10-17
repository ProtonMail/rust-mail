use crate::values::text::Text;
use crate::values::text_list::{is_text_list_value, TextList};

#[test]
fn text_list_struct() {
    let text_list =
        TextList::new_from_vcard("\\\\ \\, \\n \t 𝕯!+-[]~,\\\\ \\, \\n \t 𝕯!+-[]~").unwrap();
    assert_eq!(text_list.0.len(), 2);
    assert_eq!(text_list.0[0], Text::new_unchecked("\\ , \n \t 𝕯!+-[]~"));
    assert_eq!(text_list.0[1], Text::new_unchecked("\\ , \n \t 𝕯!+-[]~"));
}

#[test]
fn text_list_value() {
    assert!(is_text_list_value(
        "\\\\ \\, \\n \t 𝕯!+-[]~,\\\\ \\, \\n \t 𝕯!+-[]~"
    ));
    assert!(is_text_list_value(r"foo\,bar"));
    assert!(is_text_list_value(""));
    assert!(!is_text_list_value(r"foo\"));
    assert!(is_text_list_value(r"foo,"));
    assert!(!is_text_list_value("\\"));
    assert!(!is_text_list_value("\n"));
}
