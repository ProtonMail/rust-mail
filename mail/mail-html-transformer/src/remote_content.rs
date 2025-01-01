#![allow(clippy::must_use_candidate)]
//! This pass focuses on blocking remote content from loading and/or patching remote content Urls to
//! go through the Proton Proxy.
//!
//! Since these are use configurable options, each of these has a separate pass which undoes the
//! changes.

#[cfg(test)]
#[path = "tests/remote_content.rs"]
mod tests;

use html5ever::{namespace_url, ns, Namespace};
use kuchikiki::iter::NodeEdge;
use kuchikiki::{ExpandedName, NodeRef};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Url: {0}")]
    Url(#[from] url::ParseError),
}

const WHITELISTED_ELEMENTS: [&str; 3] = ["a", "base", "area"];

const PROTON_PREFIX: &str = "proton-";

/// Disable all remote content by prefixing known attributes with `proton-`.
///
/// To reverse this pass, see [`undo_disable_remote_content()`].
///
/// # Example
///
/// This will convert:
///
/// ``` html
/// <img src="...">
/// ```
/// Into:
///
/// ``` html
/// <img proton-src="...">
/// ```
///
/// # Errors
///
/// Returns an error if the selector failed to build.
pub fn disable_remote_content(document: &NodeRef) -> u64 {
    // Unfortunately the selector library does not allow use to query attributes that are not part
    // of the html standard. Attributes such as 'xlink:href` need to handled manually, so
    // we need to traverse the document manually and check each attribute ourselves.
    let attribute_list = AttributeInfo::default_list();

    let mut count = 0;
    for node in document.traverse_inclusive() {
        let NodeEdge::Start(node_ref) = node else {
            continue;
        };

        let Some(element) = node_ref.as_element() else {
            continue;
        };

        if WHITELISTED_ELEMENTS.contains(&element.name.local.as_ref()) {
            continue;
        }

        let mut attributes = element.attributes.borrow_mut();

        for item in &attribute_list {
            let Some(attribute) = attributes.map.remove(&item.enabled) else {
                continue;
            };

            attributes.map.insert(item.disabled.clone(), attribute);
            count += 1;
        }
    }
    count
}

/// Re-enables all disabled content by stripping the `proton-` prefix.
///
/// This pass does the opposite of [`disable_remote_content()`].
///
/// # Example
///
/// This will convert:
///
/// ``` html
/// <img proton-src="...">
/// ```
/// Into:
///
/// ``` html
/// <img src="...">
/// ```
pub fn undo_disable_remote_content(document: &NodeRef) {
    // Unfortunately the selector library does not allow use to query attributes that are not part
    // of the html standard. Attributes such as 'xlink:href` need to handled manually, so
    // we need to traverse the document manually and check each attribute ourselves.
    let attribute_list = AttributeInfo::default_list();

    for node in document.traverse_inclusive() {
        let NodeEdge::Start(node_ref) = node else {
            continue;
        };

        let Some(element) = node_ref.as_element() else {
            continue;
        };

        if WHITELISTED_ELEMENTS.contains(&element.name.local.as_ref()) {
            continue;
        }

        let mut attributes = element.attributes.borrow_mut();

        for item in &attribute_list {
            let Some(attribute) = attributes.map.remove(&item.disabled) else {
                continue;
            };

            attributes.map.insert(item.enabled.clone(), attribute);
        }
    }
}

/// Details on how the attributes should be represented when enabled or disabled.
struct AttributeInfo {
    /// Value of the attribute if it is enabled.
    enabled: ExpandedName,
    /// Value of the attribute if it is disabled.
    disabled: ExpandedName,
}

impl AttributeInfo {
    /// Generate a new instance with `namespace` and `value`.
    pub fn new(namespace: Namespace, value: &str) -> Self {
        Self {
            enabled: ExpandedName::new(namespace.clone(), value),
            disabled: ExpandedName::new(namespace, format! {"{PROTON_PREFIX}{value}"}),
        }
    }

    /// Generate a custom tailored replacement for `xlink:href` attributes.
    ///
    /// This need to be handled differently since the parser does not recognize the patched
    /// version as being a member of the `xlink` namespace.
    pub fn xlink_href() -> Self {
        Self {
            enabled: ExpandedName::new(ns!(xlink), "href"),
            disabled: ExpandedName::new(ns!(), format! {"xlink:{PROTON_PREFIX}href"}),
        }
    }

    /// Default list of attributes we need to patch.
    fn default_list() -> Vec<AttributeInfo> {
        vec![
            AttributeInfo::new(ns!(), "url"),
            AttributeInfo::xlink_href(),
            AttributeInfo::new(ns!(), "src"),
            AttributeInfo::new(ns!(), "srcset"),
            AttributeInfo::new(ns!(), "svg"),
            AttributeInfo::new(ns!(), "background"),
            AttributeInfo::new(ns!(), "poster"),
            AttributeInfo::new(ns!(), "data-src"),
            AttributeInfo::new(ns!(), "href"),
        ]
    }
}
