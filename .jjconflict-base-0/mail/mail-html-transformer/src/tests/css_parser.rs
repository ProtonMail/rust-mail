use lightningcss::printer::PrinterOptions;

use crate::css_parser::parse_stylesheet;

#[test]
fn handle_malformed_declaration() {
    let mut input = "*{color: red;important;background: yellow;}".to_string();
    let result = parse_stylesheet(&mut input);
    let stylesheet = result.unwrap();
    let printed = stylesheet.to_css(PrinterOptions::default()).unwrap().code;

    assert!(printed.contains("red"));
    assert!(printed.contains("#ff0"));
}
