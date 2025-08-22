use html2text::render::{PlainDecorator, TextDecorator};
use std::io::Read;

const COLUMN_WIDTH: usize = 80;

#[derive(Clone, Debug, Default)]
pub struct Html2TextOptions {
    pub decorate_links: bool,
    pub decorate_images: bool,
}

pub fn html2text(reader: impl Read, options: Html2TextOptions) -> Result<String, html2text::Error> {
    let mut config = html2text::config::with_decorator(Decorator {
        parent: PlainDecorator::new(),
        decorate_links: options.decorate_links,
        decorate_images: options.decorate_images,
    })
    .do_decorate();

    config = config.link_footnotes(options.decorate_links);

    config.string_from_read(reader, COLUMN_WIDTH).map(|s| {
        if options.decorate_images {
            s
        } else {
            //TODO(ET-4282): Returning empty string does not introduce a new line. There seems to
            // no way to insert a new line at the time being. So we manually remove
            // our custom tags for the time being.

            // Web inserts this byte sequence where the image was supposed to be.
            // Without it we can't correctly replace the signatures generated on web.
            s.replace(
                PROTON_IMAGE_DECORATION,
                str::from_utf8(&[194, 160]).expect("should not fail"),
            )
        }
    })
}

#[derive(Clone, Debug)]
struct Decorator {
    parent: PlainDecorator,
    decorate_links: bool,
    decorate_images: bool,
}
const PROTON_IMAGE_DECORATION: &str = "[proton-image-replace]";
#[allow(clippy::semicolon_if_nothing_returned)]
impl TextDecorator for Decorator {
    type Annotation = <PlainDecorator as TextDecorator>::Annotation;

    fn decorate_link_start(&mut self, url: &str) -> (String, Self::Annotation) {
        if self.decorate_links {
            self.parent.decorate_link_start(url)
        } else {
            (String::new(), ())
        }
    }

    fn decorate_link_end(&mut self) -> String {
        if self.decorate_links {
            self.parent.decorate_link_end()
        } else {
            String::new()
        }
    }

    fn decorate_em_start(&self) -> (String, Self::Annotation) {
        self.parent.decorate_em_start()
    }

    fn decorate_em_end(&self) -> String {
        self.parent.decorate_em_end()
    }

    fn decorate_strong_start(&self) -> (String, Self::Annotation) {
        self.parent.decorate_strong_start()
    }

    fn decorate_strong_end(&self) -> String {
        self.parent.decorate_strong_end()
    }

    fn decorate_strikeout_start(&self) -> (String, Self::Annotation) {
        self.parent.decorate_strikeout_start()
    }

    fn decorate_strikeout_end(&self) -> String {
        self.parent.decorate_strikeout_end()
    }

    fn decorate_code_start(&self) -> (String, Self::Annotation) {
        self.parent.decorate_code_start()
    }

    fn decorate_code_end(&self) -> String {
        self.parent.decorate_code_end()
    }

    fn decorate_preformat_first(&self) -> Self::Annotation {
        self.parent.decorate_preformat_first()
    }

    fn decorate_preformat_cont(&self) -> Self::Annotation {
        self.parent.decorate_preformat_cont()
    }

    fn decorate_image(&mut self, src: &str, title: &str) -> (String, Self::Annotation) {
        if self.decorate_images {
            self.parent.decorate_image(src, title)
        } else {
            (String::from(PROTON_IMAGE_DECORATION), ())
        }
    }

    fn header_prefix(&self, level: usize) -> String {
        self.parent.header_prefix(level)
    }

    fn quote_prefix(&self) -> String {
        self.parent.quote_prefix()
    }

    fn unordered_item_prefix(&self) -> String {
        self.parent.unordered_item_prefix()
    }

    fn ordered_item_prefix(&self, i: i64) -> String {
        self.parent.ordered_item_prefix(i)
    }

    fn make_subblock_decorator(&self) -> Self {
        self.clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{fs, io::Cursor};

    #[test]
    fn with_decorate_links() {
        let input = fs::read_to_string("src/tests/html/smoke.html").unwrap();

        let output = html2text(
            Cursor::new(input),
            Html2TextOptions {
                decorate_links: true,
                ..Default::default()
            },
        )
        .unwrap();

        insta::assert_snapshot!(output);
    }

    #[test]
    fn without_decorate_links() {
        let input = fs::read_to_string("src/tests/html/smoke.html").unwrap();

        let output = html2text(
            Cursor::new(input),
            Html2TextOptions {
                decorate_links: false,
                ..Default::default()
            },
        )
        .unwrap();

        insta::assert_snapshot!(output);
    }

    #[test]
    fn decorate_image_blocks_disabled() {
        let input = fs::read_to_string("src/tests/html/signature.html").unwrap();
        let output = html2text(
            Cursor::new(input),
            Html2TextOptions {
                decorate_links: false,
                decorate_images: false,
            },
        )
        .unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn decorate_image_blocks_enabled() {
        let input = fs::read_to_string("src/tests/html/signature.html").unwrap();
        let output = html2text(
            Cursor::new(input),
            Html2TextOptions {
                decorate_links: false,
                decorate_images: true,
            },
        )
        .unwrap();
        insta::assert_snapshot!(output);
    }
}
