//! iOS specific transformations required to correctly display content in the
//! OS's web view.

use crate::Error;
use html5ever::{namespace_url, ns, QualName};
use kuchikiki::{Attributes, ElementData, NodeData, NodeRef};
use std::cell::RefCell;

/// This pass injects a `meta` element into the HTML `head` element.
///
/// This is currently required to ensure the iOS web view resizes to fit the
/// content being displayed in Swift UI.
///
/// This will inject the following snippet into the `head` element of the
/// document.
/// ```html
/// <meta name="viewport" content="width=device-width, initial-scale=1.0">
/// ```
///
/// # Errors
///
/// Returns error if we could not find the `head` element in the document.
pub fn inject_content_size(document: &NodeRef) -> Result<(), Error> {
    let element = document
        .select_first("head")
        .map_err(|()| Error::HeadElementNotFound)?;

    let mut attributes = Attributes {
        // need to include another crate otherwise.
        #[allow(clippy::default_trait_access)]
        map: Default::default(),
    };
    attributes.insert("name", "viewport".to_owned());
    attributes.insert(
        "content",
        "width=device-width, initial-scale=1.0".to_owned(),
    );

    let data = ElementData {
        name: QualName::new(None, ns!(), "meta".into()),
        attributes: RefCell::new(attributes),
        template_contents: None,
    };

    element
        .as_node()
        .append(NodeRef::new(NodeData::Element(data)));

    Ok(())
}

#[test]
fn inject_with_existing_head_element() {
    let input = r"<html><head></head><body></body></html>";

    let expected = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=1.0"></meta></head><body></body></html>"#;

    let mut transformer = crate::Transformer::new(input);
    transformer.inject_ios_content_size().unwrap();
    let output = transformer.to_string();
    assert_eq!(expected, output);
}
#[test]
fn inject_without_existing_head_element() {
    let input = r"<html><body></body></html>";

    let expected = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=1.0"></meta></head><body></body></html>"#;

    let mut transformer = crate::Transformer::new(input);
    transformer.inject_ios_content_size().unwrap();
    let output = transformer.to_string();
    assert_eq!(expected, output);
}

#[test]
fn inject_without_existing_viewport_entry() {
    // Make sure it appears as the last entry if an existing meta item already exist.
    let input = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=0.0"></head><body></body></html>"#;

    // The parser outputs a closing meta tag only for the newly added element. Existing meta
    // elements do not have this issue.
    let expected = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=0.0"><meta name="viewport" content="width=device-width, initial-scale=1.0"></meta></head><body></body></html>"#;

    let mut transformer = crate::Transformer::new(input);
    transformer.inject_ios_content_size().unwrap();
    let output = transformer.to_string();
    assert_eq!(expected, output);
}
