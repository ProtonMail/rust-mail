#![allow(clippy::must_use_candidate)]

#[cfg(test)]
#[path = "tests/remote_content.rs"]
mod tests;

use crate::css_parser::{parse_style_attribute, parse_stylesheet};
use html5ever::namespace_url;
use html5ever::ns;
use kuchikiki::iter::NodeEdge;
use kuchikiki::{Attribute, NodeRef};
use kuchikiki::{ExpandedName, NodeData};
use lightningcss::printer::PrinterOptions;
use lightningcss::properties::custom::Function;
use lightningcss::values::url::Url;
use lightningcss::visitor::{Visit, VisitTypes, Visitor};
use std::collections::HashSet;
use std::convert::Infallible;
use tracing::warn;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Url: {0}")]
    Url(#[from] url::ParseError),
}

#[derive(Default)]
pub struct RemoteContentOutput {
    pub remote_urls: HashSet<String>,
    pub embedded_urls: HashSet<String>,
}

pub fn remote_content(
    document: &NodeRef,
    hide_remote: bool,
    hide_embedded: bool,
) -> RemoteContentOutput {
    if !hide_remote && !hide_embedded {
        return RemoteContentOutput::default();
    }

    let mut remote_urls = HashSet::new();
    let mut embedded_urls = HashSet::new();
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
        ExpandedName::new("", "action"),
        ExpandedName::new("", "formaction"),
        ExpandedName::new("", "cite"),
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
                    let out =
                        handle_style_sheet(&mut text.borrow_mut(), hide_remote, hide_embedded);
                    remote_urls.extend(out.remote_urls);
                    embedded_urls.extend(out.embedded_urls);
                }
            });
        }

        // These do not contain remote content.
        if hide_remote && ["a", "base", "area"].contains(&element.name.local.as_ref()) {
            continue;
        }

        let mut attributes = element.attributes.borrow_mut();

        for item in &attrs {
            let Some(attr) = attributes.map.get_mut(item) else {
                continue;
            };

            match is_embedded_url(attr) {
                Ok(true) => {
                    embedded_urls.insert(attr.value.clone());
                    if hide_embedded {
                        attr.value = String::new();
                    }
                }
                Ok(false) => {
                    remote_urls.insert(attr.value.clone());
                    if hide_remote {
                        attr.value = String::new();
                    }
                }
                Err(_) => {
                    remote_urls.insert(attr.value.clone());
                    attr.value = String::new();
                }
            }
        }

        // Check css styles
        if should_check_css && let Some(attr) = attributes.map.get_mut(&style_attribute) {
            let out = handle_style_attribute(&mut attr.value, hide_remote, hide_embedded);
            remote_urls.extend(out.remote_urls);
            embedded_urls.extend(out.embedded_urls);
        }
    }

    RemoteContentOutput {
        remote_urls,
        embedded_urls,
    }
}

fn is_embedded_url(attr: &Attribute) -> Result<bool, url::ParseError> {
    is_embedded_url_str(&attr.value)
}

fn is_embedded_url_str(uri: &str) -> Result<bool, url::ParseError> {
    let uri = url::Url::parse(uri)?;
    let scheme = uri.scheme();
    Ok(scheme.eq_ignore_ascii_case("cid") ||
        // We disable data: because otherwise the clients might freak out
        // If at some point we treat PGP inline attachments different revisit this.
        scheme.eq_ignore_ascii_case("data"))
}
fn handle_style_sheet(
    css: &mut String,
    disable_remote: bool,
    disable_embedded: bool,
) -> RemoteContentOutput {
    let Ok(mut sheet) = parse_stylesheet(css).inspect_err(|e| {
        warn!("StyleSheet parsing failed: {}", e);
    }) else {
        return RemoteContentOutput::default();
    };

    let mut visitor = CssUrlVisitor::new(disable_remote, disable_embedded);

    let _ = sheet.visit(&mut visitor);

    if !visitor.has_changes {
        return RemoteContentOutput {
            remote_urls: visitor.remote_urls,
            embedded_urls: visitor.embedded_urls,
        };
    }

    let Ok(patched) = sheet.to_css(PrinterOptions::default()) else {
        warn!("Failed to convert style sheet to css value");
        return RemoteContentOutput::default();
    };

    drop(sheet);

    *css = patched.code;

    RemoteContentOutput {
        remote_urls: visitor.remote_urls,
        embedded_urls: visitor.embedded_urls,
    }
}

fn handle_style_attribute(
    css: &mut String,
    disable_remote: bool,
    disable_embedded: bool,
) -> RemoteContentOutput {
    let Ok(mut style_attribute) = parse_style_attribute(css).inspect_err(|e| {
        warn!("Style attribute parsing failed: {}", e);
    }) else {
        return RemoteContentOutput::default();
    };

    let mut visitor = CssUrlVisitor::new(disable_remote, disable_embedded);

    let _ = style_attribute.visit(&mut visitor);

    if !visitor.has_changes {
        return RemoteContentOutput {
            remote_urls: visitor.remote_urls,
            embedded_urls: visitor.embedded_urls,
        };
    }

    let Ok(patched) = style_attribute.to_css(PrinterOptions::default()) else {
        warn!("Failed to convert style attribute to css value");
        return RemoteContentOutput::default();
    };

    drop(style_attribute);

    *css = patched.code;

    RemoteContentOutput {
        remote_urls: visitor.remote_urls,
        embedded_urls: visitor.embedded_urls,
    }
}

struct CssUrlVisitor {
    has_changes: bool,
    disable_remote: bool,
    disable_embedded: bool,
    remote_urls: HashSet<String>,
    embedded_urls: HashSet<String>,
}

impl CssUrlVisitor {
    fn new(disable_remote: bool, disable_embedded: bool) -> CssUrlVisitor {
        Self {
            has_changes: false,
            disable_remote,
            disable_embedded,
            remote_urls: HashSet::new(),
            embedded_urls: HashSet::new(),
        }
    }
}

impl<'i> Visitor<'i> for CssUrlVisitor {
    type Error = Infallible;

    fn visit_types(&self) -> VisitTypes {
        VisitTypes::URLS | VisitTypes::FUNCTIONS | VisitTypes::IMAGES
    }

    fn visit_url(&mut self, url: &mut Url<'i>) -> Result<(), Self::Error> {
        match is_embedded_url_str(&url.url) {
            Ok(true) => {
                self.embedded_urls.insert(url.url.to_string());
                if self.disable_embedded {
                    url.url = String::new().into();
                    self.has_changes = true;
                }
            }
            Ok(false) => {
                self.remote_urls.insert(url.url.to_string());
                if self.disable_remote {
                    url.url = String::new().into();
                    self.has_changes = true;
                }
            }
            Err(_) => {
                self.remote_urls.insert(url.url.to_string());
                url.url = String::new().into();
                self.has_changes = true;
            }
        }
        Ok(())
    }

    fn visit_function(&mut self, function: &mut Function<'i>) -> Result<(), Self::Error> {
        if function.name.to_lowercase() == "image-set" {
            function.arguments.0.clear();
            function.name = "proton-image-set".into();
            self.has_changes = true;
        }
        function.visit_children(self)
    }
}
