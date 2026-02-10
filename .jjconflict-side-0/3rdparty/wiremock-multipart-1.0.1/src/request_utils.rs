use std::str::FromStr;

use wiremock::http::HeaderName;
use wiremock::Request;

use crate::part::Part;

pub trait RequestUtils {
    fn multipart_contenttype(&self) -> Option<MultipartContentType>;
    fn parts(&self) -> Vec<Part>;
}

impl RequestUtils for Request {
    fn multipart_contenttype(&self) -> Option<MultipartContentType> {
        let content_type = self
            .headers
            .get_all(&HeaderName::from_str("content-type").unwrap())
            .iter()
            .find(|value| {
                value
                    .to_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .starts_with("multipart/")
            });
        match content_type {
            None => None,
            Some(value) => {
                let parts = value
                    .to_str()
                    .unwrap_or_default()
                    .split(";")
                    .collect::<Vec<_>>();

                let multipart_type = parts[0].split("/").nth(1).unwrap().trim();

                let boundary = parts
                    .iter()
                    .map(|part| part.trim())
                    .find(|part| part.starts_with("boundary="))
                    .map(|whole| whole.split("=").nth(1).unwrap().trim());

                Some(MultipartContentType {
                    multipart_type,
                    boundary,
                })
            }
        }
    }

    fn parts(&self) -> Vec<Part> {
        if let Some(content_type) = self.multipart_contenttype() {
            if content_type.multipart_type == "form-data" {
                if let Some(boundary) = content_type.boundary {
                    let boundary = {
                        let mut tmp: Vec<u8> = vec!['-' as u8; boundary.as_bytes().len() + 2];
                        tmp[0] = '-' as u8;
                        tmp[1] = '-' as u8;
                        tmp[2..].copy_from_slice(boundary.as_bytes());
                        tmp
                    };

                    let boundary_start_indexes = self
                        .body
                        .windows(boundary.len())
                        .enumerate()
                        .filter(|(_, window)| window == &boundary)
                        .map(|(index, _)| index)
                        .collect::<Vec<_>>();

                    boundary_start_indexes
                        .windows(2)
                        .map(|w| (boundary.len() + 1 + w[0], w[1]))
                        .map(|(start, end)| &self.body[start..end])
                        .map(|body| trim_single_linebreak_from_start(body))
                        .map(|body| trim_single_linebreak_from_end(body))
                        .map(|it| Part::from(it))
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }
}

fn trim_single_linebreak_from_start(body: &[u8]) -> &[u8] {
    if body.len() >= 2 {
        match body[..2] {
            [b'\r', b'\n'] => &body[2..],
            [b'\n', _] => &body[1..],
            _ => body,
        }
    } else if body.len() >= 1 {
        match body[0] {
            b'\n' => &body[1..],
            _ => body,
        }
    } else {
        body
    }
}

fn trim_single_linebreak_from_end(body: &[u8]) -> &[u8] {
    if body.len() >= 2 {
        match body[(body.len() - 2)..(body.len())] {
            [b'\r', b'\n'] => &body[..body.len() - 2],
            [_, b'\n'] => &body[..body.len() - 1],
            _ => body,
        }
    } else if body.len() >= 1 {
        match body[(body.len() - 1)..(body.len())] {
            [_, b'\n'] => &body[..body.len() - 1],
            _ => body,
        }
    } else {
        body
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct MultipartContentType<'a> {
    pub multipart_type: &'a str,
    pub boundary: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use maplit::hashmap;

    use crate::test_utils::{multipart_header, name, request, requestb, values};

    use super::*;

    #[test]
    fn multipart_contenttype_should_return_none_if_no_multipart_request() {
        assert_eq!(request(hashmap! {},).multipart_contenttype(), None);

        assert_eq!(
            request(hashmap! {
                name("accept") => values("application/json"),
            },)
            .multipart_contenttype(),
            None
        );

        assert_eq!(
            request(hashmap! {
                name("content-type") => values("image/jpeg"),
            },)
            .multipart_contenttype(),
            None
        );
    }

    #[test]
    fn multipart_contenttype_should_return_some_if_multipart_request() {
        assert_eq!(
            request(hashmap! {
                name("content-type") => values("multipart/foo"),
            },)
            .multipart_contenttype(),
            Some(MultipartContentType {
                multipart_type: "foo",
                boundary: None,
            })
        );

        assert_eq!(
            request(hashmap! {
                name("content-type") => values("multipart/bar; boundary=xyz"),
            },)
            .multipart_contenttype(),
            Some(MultipartContentType {
                multipart_type: "bar",
                boundary: Some("xyz"),
            })
        );
    }

    #[test]
    fn parts_should_find_single_text_part() {
        assert_eq!(
            requestb(
                hashmap! {
                    name("content-type") => values("multipart/form-data; boundary=xyz"),
                },
                indoc! {"
                    --xyz
                    Content-Disposition: form-data; name=\"part1\"

                    content
                    --xyz--
                "}
                .as_bytes()
                .into(),
            )
            .parts(),
            vec![Part::from(
                "Content-Disposition: form-data; name=\"part1\"\n\ncontent"
            ),],
        );
    }

    #[test]
    fn parts_should_find_single_text_part_with_crnr() {
        assert_eq!(
            requestb(
                multipart_header(),
                "--xyz\r\nContent-Disposition: form-data; name=\"part1\"\r\n\r\ncontent\r\n--xyz--"
                    .as_bytes()
                    .into()
            )
            .parts(),
            vec![Part::from(
                "Content-Disposition: form-data; name=\"part1\"\r\n\r\ncontent"
            ),],
        );
    }

    #[test]
    fn parts_should_find_two_text_parts() {
        assert_eq!(
            requestb(
                hashmap! {
                    name("content-type") => values("multipart/form-data; boundary=xyz"),
                },
                indoc! {r#"
                    --xyz
                    Content-Disposition: form-data; name="part1"

                    content
                    --xyz
                    Content-Disposition: form-data; name="file"; filename="Cargo.toml"
                    Content-Type: plain/text

                    [workspace]
                    members = [
                        "fhttp",
                        "fhttp-core",
                    ]

                    --xyz--
                "#}
                .as_bytes()
                .into(),
            )
            .parts(),
            vec![
                Part::from("Content-Disposition: form-data; name=\"part1\"\n\ncontent"),
                Part::from(indoc! {r#"
                    Content-Disposition: form-data; name="file"; filename="Cargo.toml"
                    Content-Type: plain/text

                    [workspace]
                    members = [
                        "fhttp",
                        "fhttp-core",
                    ]
                "#}),
            ],
        );
    }

    #[test]
    fn parts_should_find_two_text_parts_with_crnl() {
        let part2 = {
            let mut tmp = String::new();
            tmp += "Content-Disposition: form-data; name=\"file\"; filename=\"Cargo.toml\"";
            tmp += "\r\nContent-Type: plain/text";
            tmp += "\r\n\r\n";
            tmp += "[workspace]\nmembers = [\n    \"fhttp\",\n    \"fhttp-core\",\n]\n";

            tmp
        };
        let mut body = String::new();
        body += "--xyz";
        body += "\r\nContent-Disposition: form-data; name=\"part1\"";
        body += "\r\n\r\ncontent";
        body += "\r\n--xyz\r\n";
        body += &part2;
        body += "\r\n--xyz--\r\n";

        assert_eq!(
            requestb(multipart_header(), body.as_bytes().into(),).parts(),
            vec![
                Part::from("Content-Disposition: form-data; name=\"part1\"\r\n\r\ncontent"),
                Part::from(&part2),
            ],
        );
    }
}
