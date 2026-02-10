#![allow(clippy::must_use_candidate)]

#[cfg(test)]
#[path = "tests/remote_content.rs"]
mod tests;

use crate::css_parser::{parse_style_attribute, parse_stylesheet};
use html5ever::LocalName;
use html5ever::namespace_url;
use html5ever::ns;
use kuchikiki::Attributes;
use kuchikiki::ExpandedName;
use kuchikiki::NodeData;
use kuchikiki::iter::NodeEdge;
use kuchikiki::{Attribute, NodeRef};
use lightningcss::printer::PrinterOptions;
use lightningcss::properties::custom::Function;
use lightningcss::values::url::Url;
use lightningcss::visitor::{Visit, VisitTypes, Visitor};
use std::cell::RefMut;
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::LazyLock;
use tracing::warn;
use velcro::hash_set;

static ATTRIBUTES_TO_CHECK: LazyLock<HashSet<ExpandedName>> = LazyLock::new(|| {
    hash_set![
        crate::utils::attribute_name("url"),
        crate::utils::attribute_name("src"),
        crate::utils::attribute_name("srcset"),
        crate::utils::attribute_name("svg"),
        crate::utils::attribute_name("background"),
        crate::utils::attribute_name("poster"),
        crate::utils::attribute_name("data-src"),
        crate::utils::attribute_name("href"),
        crate::utils::attribute_name("action"),
        crate::utils::attribute_name("formaction"),
        crate::utils::attribute_name("cite"),
        crate::utils::attribute_name_ex(ns!(xlink), "href"),
    ]
});

static LINK_LIKE_TAGS: LazyLock<HashSet<LocalName>> = LazyLock::new(|| {
    hash_set![
        LocalName::from("a"),
        LocalName::from("area"),
        LocalName::from("base"),
    ]
});

static STYLE_ATTR: LazyLock<ExpandedName> = LazyLock::new(|| crate::utils::attribute_name("style"));
static STYLE_NODE: LazyLock<LocalName> = LazyLock::new(|| LocalName::from("style"));

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Url: {0}")]
    Url(#[from] url::ParseError),
}

#[derive(Default, Debug)]
pub struct RemoteContentOutput {
    pub remote_urls: HashSet<String>,
    pub embedded_urls: HashSet<String>,
}

pub fn remote_content(
    document: &NodeRef,
    hide_remote: bool,
    hide_embedded: bool,
) -> RemoteContentOutput {
    let mut remote_urls = HashSet::new();
    let mut embedded_urls = HashSet::new();

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

        if element.name.local == *STYLE_NODE {
            node_ref.children().for_each(|child| {
                if let NodeData::Text(text) = child.data() {
                    let out =
                        handle_style_sheet(&mut text.borrow_mut(), hide_remote, hide_embedded);
                    remote_urls.extend(out.remote_urls);
                    embedded_urls.extend(out.embedded_urls);
                }
            });
        }

        let mut attributes = element.attributes.borrow_mut();

        if !LINK_LIKE_TAGS.contains(&element.name.local) {
            check_for_remote_content_in_attributes(
                &mut attributes,
                &mut remote_urls,
                &mut embedded_urls,
                hide_embedded,
                hide_remote,
            );
        }

        // Check css styles
        if let Some(attr) = attributes.map.get_mut(&*STYLE_ATTR) {
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

fn check_for_remote_content_in_attributes(
    attributes: &mut RefMut<'_, Attributes>,
    remote_urls: &mut HashSet<String>,
    embedded_urls: &mut HashSet<String>,
    hide_embedded: bool,
    hide_remote: bool,
) {
    for item in ATTRIBUTES_TO_CHECK.iter() {
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
        VisitTypes::all()
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
