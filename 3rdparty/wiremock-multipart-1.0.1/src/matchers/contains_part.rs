use std::borrow::Cow;

use wiremock::{Match, Request};

use crate::request_utils::RequestUtils;

/// Matcher builder to assert the presence of a matching part in the request.
///
/// ## Example
///
/// ```rust
/// use wiremock::{MockServer, Mock, ResponseTemplate};
/// use wiremock::matchers::method;
/// use wiremock_multipart::prelude::*;
///
/// #[async_std::main]
/// async fn main() {
///     // Start a background HTTP server on a random local port
///     let mock_server = MockServer::start().await;
///
///     Mock::given(method("POST"))
///         .and(ContainsPart::new()
///             .with_name("data-part")
///             .with_content_type("text/plain")
///             .with_body("simple text".as_bytes()))
///         .respond_with(ResponseTemplate::new(200))
///         .mount(&mock_server)
///         .await;
/// }
/// ```
#[derive(Default, Debug, PartialEq, Eq)]
pub struct ContainsPart<'a, 'b, 'c, 'd> {
    pub name: Option<Cow<'a, str>>,
    pub filename: Option<Cow<'b, str>>,
    pub content_type: Option<Cow<'c, str>>,
    pub body: Option<Cow<'d, [u8]>>,
}

impl<'a, 'b, 'c, 'd> ContainsPart<'a, 'b, 'c, 'd> {
    pub fn new() -> Self { Self::default() }

    pub fn with_name<T: Into<Cow<'a, str>>>(self, name: T) -> Self {
        ContainsPart {
            name: Some(name.into()),
            ..self
        }
    }

    pub fn with_filename<T: Into<Cow<'b, str>>>(self, filename: T) -> Self {
        ContainsPart {
            filename: Some(filename.into()),
            ..self
        }
    }

    pub fn with_content_type<T: Into<Cow<'c, str>>>(self, content_type: T) -> Self {
        ContainsPart {
            content_type: Some(content_type.into()),
            ..self
        }
    }

    pub fn with_body<T: Into<Cow<'d, [u8]>>>(self, body: T) -> Self {
        ContainsPart {
            body: Some(body.into()),
            ..self
        }
    }
}

impl<'a, 'b, 'c, 'd> Match for ContainsPart<'a, 'b, 'c, 'd> {
    fn matches(&self, request: &Request) -> bool {
        request.parts().iter()
            .any(|part| {
                let name = self.name.as_ref()
                    .map(|required_name| {
                        part.name()
                            .map(|part_name| required_name == part_name)
                            .unwrap_or(false)
                    })
                    .unwrap_or(true);

                let filename = self.filename.as_ref()
                    .map(|required_filename| {
                        part.filename()
                            .map(|part_filename| required_filename == part_filename)
                            .unwrap_or(false)
                    })
                    .unwrap_or(true);

                let content_type = self.content_type.as_ref()
                    .map(|required_content_type| {
                        part.content_type()
                            .map(|part_content_type| required_content_type == part_content_type)
                            .unwrap_or(false)
                    })
                    .unwrap_or(true);

                let body = self.body.as_ref()
                    .map(|required_body| {
                        part.body()
                            .map(|part_body| required_body.as_ref() == part_body)
                            .unwrap_or(false)
                    })
                    .unwrap_or(true);

                name && filename && content_type && body
            })
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use maplit::hashmap;

    use crate::test_utils::{multipart_header, name, requestb, values};

    use super::*;

    #[test]
    fn default_should_be_all_none() {
        assert_eq!(
            ContainsPart::default(),
            ContainsPart { name: None, filename: None, content_type: None, body: None }
        );
    }

    #[test]
    fn new_should_be_default() {
        assert_eq!(
            ContainsPart::default(),
            ContainsPart::new(),
        );
    }

    #[test]
    fn should_add_name() {
        assert_eq!(
            ContainsPart::new().with_name("name"),
            ContainsPart {
                name: Some(Cow::Borrowed("name")),
                ..Default::default()
            }
        );
    }

    #[test]
    fn should_add_filename() {
        assert_eq!(
            ContainsPart::new().with_filename("filename"),
            ContainsPart {
                filename: Some(Cow::Borrowed("filename")),
                ..Default::default()
            }
        );
    }

    #[test]
    fn should_add_content_type() {
        assert_eq!(
            ContainsPart::new().with_content_type("application/json"),
            ContainsPart {
                content_type: Some(Cow::Borrowed("application/json")),
                ..Default::default()
            }
        );
    }

    #[test]
    fn should_add_body() {
        assert_eq!(
            ContainsPart::new().with_body("the body".as_bytes()),
            ContainsPart {
                body: Some(Cow::Borrowed("the body".as_bytes())),
                ..Default::default()
            }
        );
    }

    #[test]
    fn empty_should_match_any() {
        assert_eq!(
            ContainsPart::new().matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="part"

                    content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            true
        );
    }

    #[test]
    fn empty_should_not_match_request_without_parts() {
        assert_eq!(
            ContainsPart::new().matches(
                &requestb(
                    hashmap!{
                        name("content-type") => values("text/plain"),
                    },
                    "not a multipart request".as_bytes().into(),
                ),
            ),
            false
        );
    }

    #[test]
    fn should_match_on_name() {
        assert_eq!(
            ContainsPart::new().with_name("part-a").matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="not-the-part"

                    content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            false
        );

        assert_eq!(
            ContainsPart::new().with_name("part-a").matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="part-a"

                    content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            true
        );
    }

    #[test]
    fn should_match_on_filename() {
        assert_eq!(
            ContainsPart::new().with_filename("file-a").matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="not-the-part"; filename="not-the-file"

                    content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            false
        );

        assert_eq!(
            ContainsPart::new().with_filename("file-a").matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="part-a"; filename="file-a"

                    content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            true
        );
    }

    #[test]
    fn should_match_on_content_type() {
        assert_eq!(
            ContainsPart::new().with_content_type("application/json").matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="not-the-part" filename="not-the-file"
                    Content-Type: application/xml

                    content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            false
        );

        assert_eq!(
            ContainsPart::new().with_content_type("application/json").matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="part-a" filename="file-a"
                    Content-Type: application/json

                    content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            true
        );
    }

    #[test]
    fn should_match_on_body() {
        assert_eq!(
            ContainsPart::new().with_body("content".as_bytes()).matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="not-the-part" filename="not-the-file"
                    Content-Type: application/xml

                    not the right content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            false
        );

        assert_eq!(
            ContainsPart::new().with_body("content".as_bytes()).matches(
                &requestb(
                    multipart_header(),
                    indoc!{r#"
                    --xyz
                    Content-Disposition: form-data; name="part-a" filename="file-a"
                    Content-Type: application/json

                    content
                    --xyz--
                "#}.as_bytes().into()
                ),
            ),
            true
        );
    }

}
