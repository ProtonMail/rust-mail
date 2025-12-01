//! iOS specific transformations required to correctly display content in the
//! OS's web view.

#[cfg(test)]
#[path = "tests/ios.rs"]
mod tests;

use html5ever::{QualName, namespace_url, ns};
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
pub fn inject_content_size(document: NodeRef) {
    let element = document.select_first("head").unwrap(); // kuckikiki always adds it

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
}
