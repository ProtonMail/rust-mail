//! iOS specific transformations required to correctly display content in the
//! OS's web view.

#[cfg(test)]
#[path = "tests/ios.rs"]
mod tests;

use html5ever::{QualName, namespace_url, ns};
use kuchikiki::{Attribute, ExpandedName, NodeRef};

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
    let element = document.select_first("head").unwrap_or_else(|()| {
        let head = NodeRef::new_element(
            QualName::new(None, ns!(html), "head".into()),
            std::iter::empty(),
        );
        document.append(head.clone());
        // SAFETY: We just created it using new_element, so it's safe to unwrap.
        head.into_element_ref().unwrap()
    });

    let meta = NodeRef::new_element(
        QualName::new(None, ns!(html), "meta".into()),
        [
            (
                ExpandedName::new(ns!(), "name"),
                Attribute {
                    prefix: None,
                    value: "viewport".into(),
                },
            ),
            (
                ExpandedName::new(ns!(), "content"),
                Attribute {
                    prefix: None,
                    value: "width=device-width, initial-scale=1.0".into(),
                },
            ),
        ],
    );

    element.as_node().append(meta);
}
