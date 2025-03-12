#![allow(clippy::must_use_candidate)]
//! This pass focuses on blocking remote content from loading and/or patching remote content Urls to
//! go through the Proton Proxy.
//!
//! Since these are use configurable options, each of these has a separate pass which undoes the
//! changes.

#[cfg(test)]
#[path = "tests/remote_content.rs"]
mod tests;

use html5ever::{Namespace, namespace_url, ns};
use kuchikiki::iter::NodeEdge;
use kuchikiki::{Attribute, ExpandedName, NodeRef};
use url::Url;

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

const PROXY_BASE_URL: &str = "https://mail.proton.me/api/core/v4/images";

/// Proxies all images through proton's proxy.
///
/// `session_id` must be a valid `UID`.
#[allow(clippy::missing_panics_doc)] // url parsing should not fail
pub fn proxy_images(document: NodeRef, session_id: &str) -> u64 {
    // Unfortunately the selector library does not allow use to query attributes that are not part
    // of the html standard. Attributes such as 'xlink:href` need to handled manually, so
    // we need to traverse the document manually and check each attribute ourselves.
    let attribute_list = AttributeInfo::default_list();

    let mut base = Url::parse(PROXY_BASE_URL).expect("Should always be valid");
    base.query_pairs_mut().append_pair("UID", session_id);
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

        let proxy_value = |value: &str| -> Option<String> {
            // We should not proxy cid images.
            if value.starts_with("cid:") {
                return None;
            }

            // We can't proxy embedded data.
            if value.starts_with("data:") {
                return None;
            }

            let mut new = base.clone();
            new.query_pairs_mut().append_pair("Url", value);
            Some(new.to_string())
        };

        let mut attributes = element.attributes.borrow_mut();
        for item in &attribute_list {
            if let Some(attr) = attributes.map.get_mut(&item.enabled) {
                if item.enabled.local.as_ref() == "srcset" {
                    count += handle_srcset(attr, proxy_value);
                } else if let Some(value) = proxy_value(&attr.value) {
                    attr.value = value;
                    count += 1;
                }
            }
        }
    }

    count
}

/// Undo the proxying of images through proton server.
#[allow(clippy::missing_panics_doc)] // url parsing should not fail
pub fn undo_proxy_images(document: NodeRef) -> u64 {
    // Unfortunately the selector library does not allow use to query attributes that are not part
    // of the html standard. Attributes such as 'xlink:href` need to handled manually, so
    // we need to traverse the document manually and check each attribute ourselves.
    let attribute_list = AttributeInfo::default_list();

    let base = Url::parse(PROXY_BASE_URL).expect("Should always be valid");
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

        let undo_proxy = |value: &str| -> Option<String> {
            // We should not proxy cid images
            if value.starts_with("cid:") {
                return None;
            }

            // if the data is not an url, we can't proxy it.
            let Ok(url) = Url::parse(value) else {
                return None;
            };

            if url.path() != base.path() {
                // this is not a proxied image, ignore
                return None;
            }

            for (key, value) in url.query_pairs() {
                if key == "Url" {
                    return Some(value.into());
                }
            }

            None
        };

        let mut attributes = element.attributes.borrow_mut();
        for item in &attribute_list {
            if let Some(attr) = attributes.map.get_mut(&item.enabled) {
                if item.enabled.local.as_ref() == "srcset" {
                    count += handle_srcset(attr, undo_proxy);
                } else if let Some(value) = undo_proxy(&attr.value) {
                    attr.value = value;
                    count += 1;
                }
            }
        }
    }

    count
}

/// Proxy srcset elements.
///
/// See [specification](https://developer.mozilla.org/en-US/docs/Web/API/HTMLImageElement/srcset)
/// for more details about format.
fn handle_srcset(attribute: &mut Attribute, closure: impl Fn(&str) -> Option<String>) -> u64 {
    let mut elements = Vec::new();
    let mut count = 0_u64;
    for entry in attribute.value.trim().split(',') {
        let values = entry.trim().splitn(2, ' ').collect::<Vec<&str>>();
        if values.is_empty() {
            continue;
        }
        let Some(mut new_value) = closure(values[0]) else {
            elements.push(entry.to_owned());
            continue;
        };
        if values.len() > 1 {
            new_value.push(' ');
            new_value.push_str(values[1]);
        }

        count += 1;

        elements.push(new_value);
    }

    attribute.value = elements.join(", ");
    count
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
