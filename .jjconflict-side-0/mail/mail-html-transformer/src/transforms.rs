#![allow(clippy::must_use_candidate)]
#[cfg(test)]
#[path = "tests/transforms.rs"]
mod tests;

pub mod styles;

use itertools::Itertools;
use kuchikiki::{Attribute, NodeData, NodeRef, iter::NodeEdge};
use std::fmt::Write;
use url::Url;

use crate::{utils::node_ref_from_str, utm::strip_from_url};

/// Determines which stylesheet hardcoded into the binary should be injected into HTML body of the message
///
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorMode {
    LightMode,
    DarkMode,
}

/// Moves every `<style>` node from `<head>` into `<body>`
pub fn move_styles_to_body(document: NodeRef) {
    let Ok(styles) = document.select("head style") else {
        return;
    };

    let Ok(body) = document.select_first("body") else {
        return;
    };

    // Apparently detaching and appending nodes affects this iterator
    // Therefore we need to collect all references to styles before mutating the DOM
    let styles = styles.into_iter().collect_vec();

    for style in styles {
        let style = style.as_node();
        style.detach();
        body.as_node().append(style.clone());
    }
}

/// This function overrides all `rel` attributes in `<a>` tags to be [noreferrer.](https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes/rel/noreferrer)
///
/// See [this article](https://mathiasbynens.github.io/rel-noopener/) to see how the lack of it could be abused
pub fn add_noreferrer(document: NodeRef) {
    let exp_name = crate::utils::attribute_name("rel");
    let attr = Attribute {
        prefix: None,
        value: "noreferrer".to_string(),
    };

    let Ok(anchors) = document.select("a") else {
        tracing::warn!("Could not select <a> elements");
        return;
    };

    for anchor in anchors {
        let mut attrs = anchor.attributes.borrow_mut();
        attrs.map.insert(exp_name.clone(), attr.clone());
    }
}

/// Inserts `<a>` elements in plain text links to make them clickable
pub fn insert_links(document: NodeRef) {
    let start_nodes = document.traverse_inclusive().filter_map(|node| match node {
        NodeEdge::Start(node_ref) => Some(node_ref),
        NodeEdge::End(_) => None,
    });
    // We only care about text nodes which we replace with <span> for simplicity
    let mut detach_me = vec![];
    for node_ref in start_nodes {
        let NodeData::Element(data) = node_ref.data() else {
            continue;
        };

        // This is already a link
        if &*data.name.local == "a" {
            continue;
        }
        for child in node_ref.children() {
            let NodeData::Text(text) = child.data() else {
                continue;
            };
            let Some(span) = insert_link_str(&text.borrow()) else {
                continue;
            };
            child.insert_before(span);
            detach_me.push(child);
        }
    }

    for d in detach_me {
        d.detach();
    }
}

fn insert_link_str(text: &str) -> Option<NodeRef> {
    // First pass, no allocation
    if !text.contains("http") {
        return None;
    }
    let mut rep = String::with_capacity(text.len() * 2); // TODO:(perf) reserve a bit less capacity
    for word in text.split_whitespace() {
        if word.starts_with("http")
            && let Ok(url) = Url::parse(word)
        {
            let scheme = url.scheme();
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
                let url: String = strip_from_url(url.clone()).unwrap_or(url).into();
                write!(rep, r#"<a href="{url}" rel="noreferrer">{url}</a>"#)
                    .expect("Write to complete");
                rep.push(' ');
                continue;
            }
        }
        rep.push_str(word);
        rep.push(' ');
    }
    Some(node_ref_from_str(&rep, "div"))
}

#[must_use]
/// Replaces consecutive ' ' for `&nbsp;` and '\n' for `<br>`. We also escape the `>` and `<` to
/// make sure these are not confused for valid html tags when the HTML parser finds them
/// in between other HTML tags we inject.
pub fn keep_spaces_and_escape_gt_and_lt(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    out.push_str("<pre>");

    let mut prev_was_space = false;

    for ch in text.chars() {
        match ch {
            ' ' => {
                if prev_was_space {
                    out.push_str("&nbsp;");
                } else {
                    out.push(' ');
                    prev_was_space = true;
                }
                continue;
            }
            '>' => {
                out.push_str("&gt;");
                prev_was_space = false;
                continue;
            }
            '<' => {
                out.push_str("&lt;");
                prev_was_space = false;
                continue;
            }
            _ => {}
        }
        prev_was_space = false;
        if ch == '\n' {
            out.push_str("<br>");
        } else {
            out.push(ch);
        }
    }
    out.push_str("</pre>");
    out
}
