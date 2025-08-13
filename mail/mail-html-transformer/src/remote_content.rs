#![allow(clippy::must_use_candidate)]
//! This pass focuses on blocking remote content from loading and/or patching remote content Urls to
//! go through the Proton Proxy.
//!
//! Since these are use configurable options, each of these has a separate pass which undoes the
//! changes.

#[cfg(test)]
#[path = "tests/remote_content.rs"]
mod tests;

use html5ever::namespace_url;
use html5ever::ns;
use kuchikiki::ExpandedName;
use kuchikiki::NodeRef;
use kuchikiki::iter::NodeEdge;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Url: {0}")]
    Url(#[from] url::ParseError),
}

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
pub fn disable_content(document: &NodeRef, hide_remote: bool, hide_embedded: bool) -> (u64, u64) {
    if !hide_remote && !hide_embedded {
        return (0, 0);
    }

    let mut remote_count = 0;
    let mut embedded_count = 0;

    let attrs = [
        ExpandedName::new("", "url"),
        ExpandedName::new("", "src"),
        ExpandedName::new("", "srcset"),
        ExpandedName::new("", "svg"),
        ExpandedName::new("", "background"),
        ExpandedName::new("", "poster"),
        ExpandedName::new("", "data-src"),
        ExpandedName::new("", "href"),
        ExpandedName::new(ns!(xlink), "href"),
    ];

    // Unfortunately the selector library does not allow use to query attributes that are not part
    // of the html standard. Attributes such as 'xlink:href` need to handled manually, so
    // we need to traverse the document manually and check each attribute ourselves.
    for node in document.traverse_inclusive() {
        let NodeEdge::Start(node_ref) = node else {
            continue;
        };

        let Some(element) = node_ref.as_element() else {
            continue;
        };

        // These do not contain remote content.
        if hide_remote && ["a", "base", "area"].contains(&element.name.local.as_ref()) {
            continue;
        }

        let mut attributes = element.attributes.borrow_mut();

        let mut disabled_remote = false;
        let mut disabled_embedded = false;

        for item in &attrs {
            let Some(attr) = attributes.map.get_mut(item) else {
                continue;
            };
            let attr = &mut attr.value;
            if attr.starts_with("cid:") ||
            // We disable data: because otherwise the clients might freak out
            // If at some point we treat PGP inline attachments different revisit this.
            attr.starts_with("data:")
            {
                if hide_embedded {
                    *attr = String::new();
                }
                disabled_embedded = true;
            } else {
                if hide_remote {
                    *attr = String::new();
                }
                disabled_remote = true;
            }
        }

        remote_count += u64::from(disabled_remote);
        embedded_count += u64::from(disabled_embedded);
    }
    (remote_count, embedded_count)
}
