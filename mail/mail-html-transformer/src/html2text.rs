use html2text::render::PlainDecorator;
use std::io::Read;

pub const DEFAULT_COLUMN_WIDTH: usize = 80;
pub fn convert_html_to_text(
    reader: impl Read,
    column_width: usize,
) -> Result<String, html2text::Error> {
    let config = html2text_config();
    config.string_from_read(reader, column_width)
}

fn html2text_config() -> html2text::config::Config<PlainDecorator> {
    html2text::config::plain()
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_document() {
        let input = std::fs::read_to_string("src/tests/test_document.html").unwrap();
        let output = convert_html_to_text(Cursor::new(input), 80).unwrap();
        insta::assert_snapshot!(output);
    }
    #[test]
    fn with_urls() {
        let input = r#"
 <html>
 <body> 
    My <a href="127.0.0.1"> home </a>!.
 </body>
 </html> 
        "#;
        let output = convert_html_to_text(Cursor::new(input), 80).unwrap();
        insta::assert_snapshot!(output);
    }
}
