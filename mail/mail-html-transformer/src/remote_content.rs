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
use kuchikiki::iter::NodeEdge;
use kuchikiki::{Attribute, NodeRef};
use kuchikiki::{ExpandedName, NodeData};
use lightningcss::printer::PrinterOptions;
use lightningcss::stylesheet::{ParserOptions, StyleAttribute, StyleSheet};
use lightningcss::values::url::Url;
use lightningcss::visitor::{Visit, VisitTypes, Visitor};
use std::convert::Infallible;
use tracing::warn;

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
    let should_check_css = hide_remote || hide_embedded;

    let style_attribute = ExpandedName::new("", "style");

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

        if should_check_css && element.name.local.as_ref() == "style" {
            node_ref.children().for_each(|child| {
                if let NodeData::Text(text) = child.data() {
                    handle_style_sheet(&mut text.borrow_mut(), hide_remote, hide_embedded);
                }
            });
        }

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
            if is_embedded_url(attr) {
                if hide_embedded {
                    attr.value = String::new();
                }
                disabled_embedded = true;
            } else {
                if hide_remote {
                    attr.value = String::new();
                }
                disabled_remote = true;
            }
        }

        // Check css styles
        if should_check_css && let Some(attr) = attributes.map.get_mut(&style_attribute) {
            handle_style_attribute(&mut attr.value, hide_remote, hide_embedded);
        }

        remote_count += u64::from(disabled_remote);
        embedded_count += u64::from(disabled_embedded);
    }
    (remote_count, embedded_count)
}

fn is_embedded_url(attr: &Attribute) -> bool {
    is_embedded_url_str(&attr.value)
}

fn is_embedded_url_str(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.starts_with("cid:") ||
        // We disable data: because otherwise the clients might freak out
        // If at some point we treat PGP inline attachments different revisit this.
        url_lower.starts_with("data:")
}
fn handle_style_sheet(css: &mut String, disable_remote: bool, disable_embedded: bool) {
    let Ok(mut sheet) = StyleSheet::parse(
        css,
        ParserOptions {
            error_recovery: true,
            ..Default::default()
        },
    )
    .inspect_err(|e| {
        warn!("StyleSheet parsing failed: {}", e);
    }) else {
        return;
    };

    let mut visitor = CssUrlVisitor::new(disable_remote, disable_embedded);

    let _ = sheet.visit(&mut visitor);

    if !visitor.has_changes {
        return;
    }

    let Ok(patched) = sheet.to_css(PrinterOptions::default()) else {
        warn!("Failed to convert style sheet to css value");
        return;
    };

    drop(sheet);

    *css = patched.code;
}

fn handle_style_attribute(css: &mut String, disable_remote: bool, disable_embedded: bool) {
    let Ok(mut style_attribute) = StyleAttribute::parse(
        css,
        ParserOptions {
            error_recovery: true,
            ..Default::default()
        },
    )
    .inspect_err(|e| {
        warn!("Style attribute parsing failed: {}", e);
    }) else {
        return;
    };

    let mut visitor = CssUrlVisitor::new(disable_remote, disable_embedded);

    let _ = style_attribute.visit(&mut visitor);

    if !visitor.has_changes {
        return;
    }

    let Ok(patched) = style_attribute.to_css(PrinterOptions::default()) else {
        warn!("Failed to convert style attribute to css value");
        return;
    };

    drop(style_attribute);

    *css = patched.code;
}

struct CssUrlVisitor {
    has_changes: bool,
    disable_remote: bool,
    disable_embedded: bool,
}

impl CssUrlVisitor {
    fn new(disable_remote: bool, disable_embedded: bool) -> CssUrlVisitor {
        Self {
            has_changes: false,
            disable_remote,
            disable_embedded,
        }
    }
}

impl<'i> Visitor<'i> for CssUrlVisitor {
    type Error = Infallible;

    fn visit_types(&self) -> VisitTypes {
        VisitTypes::URLS
    }

    fn visit_url(&mut self, url: &mut Url<'i>) -> Result<(), Self::Error> {
        if is_embedded_url_str(&url.url) {
            if self.disable_embedded {
                url.url = String::new().into();
                self.has_changes = true;
            }
        } else if self.disable_remote {
            url.url = String::new().into();
            self.has_changes = true;
        }
        Ok(())
    }
}
