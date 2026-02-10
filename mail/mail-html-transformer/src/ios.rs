//! iOS specific transformations required to correctly display content in the
//! OS's web view.

#[cfg(test)]
#[path = "tests/ios.rs"]
mod tests;

use kuchikiki::NodeRef;

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
    let element = crate::utils::upsert_head(&document);

    let meta = crate::utils::new_element(
        "meta",
        [
            ("name", "viewport"),
            ("content", "width=device-width, initial-scale=1.0"),
        ],
    );

    element.as_node().append(meta);
}
