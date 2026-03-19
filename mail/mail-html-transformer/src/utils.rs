use std::iter::empty;

use html5ever::{LocalName, Namespace, QualName, namespace_url, ns, tendril::TendrilSink};
use itertools::Itertools;
use kuchikiki::{Attribute, ElementData, ExpandedName, NodeDataRef, NodeRef};

/// Prefer this over `url::Url::parse()` because this function gracefully
/// handles relative urls (adds https automatically).
pub fn parse_url(input: impl AsRef<str>) -> Result<url::Url, url::ParseError> {
    let input = input.as_ref();
    let url = url::Url::parse(input);

    match url {
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let input = if input.starts_with("//") {
                // Schemaless
                format!("https:{input}")
            } else if input.starts_with('/') {
                format!("https://localhost{input}")
            } else {
                format!("https://{input}")
            };
            url::Url::parse(&input)
        }
        els => els,
    }
}

pub fn attribute_name(name: impl ToString) -> ExpandedName {
    // For some reason HTML attributes MUST not have a namespace
    ExpandedName::new(ns!(), name.to_string())
}

pub fn attribute_name_ex(ns: Namespace, name: impl ToString) -> ExpandedName {
    ExpandedName::new(ns, name.to_string())
}

pub fn new_element<K: ToString, V: ToString>(
    name: &str,
    attrs: impl IntoIterator<Item = (K, V)>,
) -> NodeRef {
    NodeRef::new_element(
        QualName::new(None, ns!(html), name.into()),
        attrs.into_iter().map(|(k, v)| {
            (
                attribute_name(k),
                Attribute {
                    prefix: None,
                    value: v.to_string(),
                },
            )
        }),
    )
}

pub fn node_ref_from_str(html: &str, tag: &str) -> NodeRef {
    let qual_name = QualName::new(None, html5ever::ns!(html), LocalName::from(tag));
    kuchikiki::parse_fragment(qual_name, vec![]).one(html)
}

pub fn upsert_head(document: &NodeRef) -> NodeDataRef<ElementData> {
    document.select_first("head").unwrap_or_else(|()| {
        let head = new_element::<&str, &str>("head", empty());
        document.append(head.clone());
        // SAFETY: We just created it using new_element, so it's safe to unwrap.
        head.into_element_ref().unwrap()
    })
}

pub fn select_all_with_attribute(
    document: &NodeRef,
    attribute_name: &str,
) -> Result<impl Iterator<Item = (NodeDataRef<ElementData>, String)>, ()> {
    let res = document
        .select(&format!("[{attribute_name}]"))
        .inspect_err(|()| {
            tracing::error!("Could not select nodes with {attribute_name} attribute");
        })?;

    Ok(res.map(move |element| {
        // SAFETY: unwrap is fine, the `.select()` ensures that the attribute exists
        let attribute = element
            .attributes
            .borrow()
            .get(attribute_name)
            .unwrap()
            .into();
        (element, attribute)
    }))
}

pub fn select_all_with_any_attribute(
    document: &NodeRef,
    attribute_names: &[&str],
) -> Result<impl Iterator<Item = NodeDataRef<ElementData>>, ()> {
    let selector = attribute_names
        .iter()
        .map(|attr| format!("[{attr}]"))
        .join(",");

    document.select(&selector).inspect_err(|()| {
        tracing::error!("Could not select nodes with any of the attributes");
    })
}

pub trait NodeRefExt {
    /// Returns all following nodes. Not just siblings (See [`NodeRef::following_siblings`])
    /// but also following "uncles" - siblings of parents
    ///
    /// Note: It does not traverse inside of those siblings and uncles.
    /// If you need such a behaviour you can do
    /// ```ignore
    ///     .following_nodes()
    ///     .flat_map(|n| n.inclusive_descendants())
    /// ```
    fn following_nodes(&self) -> impl Iterator<Item = NodeRef>;
}

impl NodeRefExt for NodeRef {
    fn following_nodes(&self) -> impl Iterator<Item = NodeRef> {
        let mut current: Option<NodeRef> = Some(self.clone());
        std::iter::from_fn(move || {
            loop {
                let c = current.take()?;
                if let Some(next) = c.next_sibling() {
                    current = Some(next.clone());
                    return Some(next);
                }
                current = c.parent();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use test_case::test_case;

    use super::*;

    #[test]
    fn fetching_all_style_attributes() {
        let html = r#"
            <html>
            <head>
            </head>
            <body style="color: red">
                <div>
                    <span>
                        <a href="http://wikipedia.com" style="background-color: yellow; color: black"> Wiki </a>
                    </span>
                </div>
            </body>
            </html>
        "#;

        let document = kuchikiki::parse_html().one(html);

        let result = select_all_with_attribute(&document, "style")
            .unwrap()
            .map(|(tag, style)| (tag.name.local.to_string(), style))
            .collect::<Vec<_>>();

        assert_eq!(
            vec![
                ("body".to_string(), "color: red".to_string()),
                (
                    "a".to_string(),
                    "background-color: yellow; color: black".to_string()
                )
            ],
            result
        );
    }

    #[test]
    fn fetching_all_deprecated_attributes() {
        let html = r#"
            <html>
            <head>
            </head>
            <body style="color: red">
                <div>
                    <span>
                        <a bgcolor="yellow"></a>
                        <span text="black"></span>
                        <marquee bgcolor="red" text="white"></marquee>
                    </span>
                </div>
            </body>
            </html>
        "#;

        let document = kuchikiki::parse_html().one(html);

        let result = select_all_with_any_attribute(&document, &["bgcolor", "text"])
            .unwrap()
            .map(|tag| tag.name.local.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            vec!["a".to_string(), "span".to_string(), "marquee".to_string(),],
            result
        );
    }

    #[test]
    fn test_following_nodes() {
        let input = r#"
            <div class="bar">
                preceding text uncle
                <div class="preceding-element-uncle"></div>
                <div class="foo">
                    preceding text sibling
                    <div class="preceding-element-sibling"></div>
                    <div class="anchor">
                        My anchor
                    </div>
                    following text sibling
                    <div class="following-element-sibling"></div>
                </div>
                following text uncle
                <div class="following-element-uncle"></div>
            </div>
        "#;

        let document = kuchikiki::parse_html().one(input);

        let anchor = document.select_first(".anchor").unwrap();

        let result = anchor
            .as_node()
            .following_nodes()
            .map(|node| node.to_string().trim().to_owned())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>();

        assert_debug_snapshot!(result);
    }

    #[test_case("http://foo.com/bar" => "http://foo.com/bar" ; "with_http")]
    #[test_case("https://foo.com/bar" => "https://foo.com/bar" ; "with_https")]
    #[test_case("foo.com/bar" => "https://foo.com/bar" ; "relative")]
    #[test_case("//foo.com/bar" => "https://foo.com/bar" ; "schemaless")]
    #[test_case("/image.png" => "https://localhost/image.png" ; "relative_with_slash")]
    #[test_case("cid://foo.com/bar" => "cid://foo.com/bar" ; "cid")]
    fn test_parse_url_roundtrip(input: &str) -> String {
        parse_url(input).unwrap().to_string()
    }
}
