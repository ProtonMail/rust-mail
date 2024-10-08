use crate::values::text::{is_text_value, Text};

#[test]
fn text_struct() {
    let text = Text::new_validated("\\\\ \\, \\n \t 𝕯!+-[]~").unwrap();
    assert_eq!(text.value, "\\ , \n \t 𝕯!+-[]~");
}

#[test]
fn text_value() {
    // text = *TEXT-CHAR
    // TEXT-CHAR = "\\" / "\," / "\n" / WSP / NON-ASCII / %x21-2B / %x2D-5B / %x5D-7E
    assert!(is_text_value("\\\\ \\, \\n \t 𝕯!+-[]~"));
    assert!(is_text_value(""));
    assert!(!is_text_value("\\"));
    assert!(!is_text_value(","));
    assert!(!is_text_value("\n"));
}
