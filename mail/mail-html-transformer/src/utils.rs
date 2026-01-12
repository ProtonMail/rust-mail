use std::iter::empty;

use html5ever::{LocalName, Namespace, QualName, namespace_url, ns, tendril::TendrilSink};
use itertools::Itertools;
use kuchikiki::{Attribute, ElementData, ExpandedName, NodeDataRef, NodeRef};

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

#[cfg(test)]
mod tests {
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
}
