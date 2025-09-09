use lazy_regex::regex;

#[derive(Debug, PartialEq, Eq)]
pub struct Part<'a> {
    pub content: &'a [u8],
}

impl<'a> Part<'a> {
    pub fn name(&self) -> Option<&'a str> {
        let header = self.header();
        match header {
            None => None,
            Some(header) => {
                let regex = regex!(r#";\s*name="([^"].*?)""#i);
                regex.captures(header)
                    .and_then(|cap| cap.get(1))
                    .map(|mtch| mtch.as_str())
            },
        }
    }

    pub fn filename(&self) -> Option<&'a str> {
        let header = self.header();
        match header {
            None => None,
            Some(header) => {
                let regex = regex!(r#";\s*filename="([^"].*?)""#i);
                regex.captures(header)
                    .and_then(|cap| cap.get(1))
                    .map(|mtch| mtch.as_str())
            },
        }
    }

    pub fn content_type(&self) -> Option<&'a str> {
        let header = self.header();
        match header {
            None => None,
            Some(header) => {
                let regex = regex!(r#"content-type:\s*([^\n].*)"#i);
                regex.captures(header)
                    .and_then(|cap| cap.get(1))
                    .map(|mtch| mtch.as_str())
            },
        }
    }

    pub fn header(&self) -> Option<&'a str> {
        match self.header_body_boundary() {
            None => None,
            Some((end_of_header_index, _)) => {
                Some(std::str::from_utf8(&self.content[0..end_of_header_index]).unwrap())
            },
        }
    }

    pub fn body(&self) -> Option<&'a [u8]> {
        match self.header_body_boundary() {
            None => None,
            Some((end_of_header_index, separator_len)) => {
                Some(&self.content[(end_of_header_index + separator_len)..])
            },
        }
    }

    fn header_body_boundary(&self) -> Option<(usize, usize)> {
        self.content
            .windows(4)
            .enumerate()
            .find_map(|(index, window)| {
                match window {
                    [b'\n', b'\n', _, _] => Some((index, 2)),
                    [b'\r', b'\n', b'\r', b'\n'] => Some((index, 4)),
                    _ => None,
                }
            })
    }
}

impl<'a> From<&'a [u8]> for Part<'a> {
    fn from(content: &'a [u8]) -> Self {
        Part {
            content,
        }
    }
}

impl<'a> From<&'a str> for Part<'a> {
    fn from(text: &'a str) -> Self {
        Part {
            content: text.as_bytes(),
        }
    }
}

impl<'a> From<&'a String> for Part<'a> {
    fn from(text: &'a String) -> Self {
        Part {
            content: text.as_bytes(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_extract_header_and_body() {
        let part = Part::from("Content-Disposition: form-data; name=\"text\"\nContent-Type: plain/text\n\ncontent");

        assert_eq!(
            part.header(),
            Some("Content-Disposition: form-data; name=\"text\"\nContent-Type: plain/text"),
        );

        assert_eq!(
            part.body(),
            Some("content".as_bytes()),
        );
    }

    #[test]
    fn should_extract_header_and_body_with_cr_and_newline() {
        let part = Part::from("Content-Disposition: form-data; name=\"text\"\r\nContent-Type: plain/text\r\n\r\ncontent");

        assert_eq!(
            part.header(),
            Some("Content-Disposition: form-data; name=\"text\"\r\nContent-Type: plain/text"),
        );

        assert_eq!(
            part.body(),
            Some("content".as_bytes()),
        );
    }

    #[test]
    fn should_extract_part_name() {
        assert_eq!(
            Part::from("Content-Disposition: form-data; name=\"text\"; filename=\"filename\"\nContent-Type: plain/text\n\ncontent").name(),
            Some("text"),
        );
    }

    #[test]
    fn should_extract_file_name() {
        assert_eq!(
            Part::from("Content-Disposition: form-data; filename=\"my-file.txt\"; name=\"text\"\n\nContent-Type: plain/text\n\ncontent").filename(),
            Some("my-file.txt"),
        );
    }

    #[test]
    fn should_extract_content_type() {
        assert_eq!(
            Part::from("Content-Disposition: form-data; name=\"text\"\n; filename=\"my-file.txt\"\nContent-Type: plain/text\n\ncontent").content_type(),
            Some("plain/text"),
        );
    }

    #[test]
    fn should_extract_part_body() {
        assert_eq!(
            Part::from("Content-Disposition: form-data; name=\"text\"\nContent-Type: plain/text\n\ncontent").body(),
            Some("content".as_bytes()),
        );
    }
}
