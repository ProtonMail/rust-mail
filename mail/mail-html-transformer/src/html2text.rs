use html2text::render::PlainDecorator;
use std::io::Read;

pub const DEFAULT_COLUMN_WIDTH: usize = 80;

pub struct Html2TextOptions {
    pub link_foot_notes: bool,
    pub column_width: usize,
}

impl Default for Html2TextOptions {
    fn default() -> Html2TextOptions {
        Self {
            link_foot_notes: true,
            column_width: DEFAULT_COLUMN_WIDTH,
        }
    }
}

pub fn convert_html_to_text(
    reader: impl Read,
    options: Html2TextOptions,
) -> Result<String, html2text::Error> {
    let mut config = html2text_config();
    if !options.link_foot_notes {
        config = config.no_link_wrapping().link_footnotes(false);
    }
    config.string_from_read(reader, options.column_width)
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
        let output = convert_html_to_text(Cursor::new(input), Html2TextOptions::default()).unwrap();
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
        let output = convert_html_to_text(Cursor::new(input), Html2TextOptions::default()).unwrap();
        insta::assert_snapshot!(output);
    }
}
