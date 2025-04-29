#![allow(clippy::must_use_candidate)]
#[cfg(test)]
#[path = "tests/transforms.rs"]
mod tests;

use html5ever::{LocalName, QualName, namespace_url, tendril::TendrilSink};
use kuchikiki::{
    Attribute, Attributes, ElementData, ExpandedName, NodeData, NodeRef, iter::NodeEdge,
};
use std::cell::RefCell;
use url::Url;

use crate::utm::strip_from_url;

fn node_ref_from_str(html: &str, tag: &str) -> NodeRef {
    let qual_name = QualName::new(None, html5ever::ns!(), LocalName::from(tag));
    kuchikiki::parse_fragment(qual_name, vec![]).one(html)
}

/// Determines which stylesheet hardcoded into the binary should be injected into HTML body of the message
///
pub enum Stylesheet {
    LightMode,
    DarkMode,
}

/// This function adds support for custom stylesheets. This fails if the html doesn't have a head tag.
/// Or if the stylesheet is not in utf-8 format.
///
/// This function will inject the following HTML snippet into the `head` tag
/// of the document:
/// ```html
/// <style>
///   ...
/// </style>
/// ```
#[allow(clippy::missing_panics_doc)]
pub fn inject_style(document: NodeRef, style: Stylesheet) {
    let element = document.select_first("head").unwrap(); // kuckikiki always adds it

    let style_text = match style {
        Stylesheet::LightMode => include_str!("light.css"),
        Stylesheet::DarkMode => include_str!("dark.css"),
    };
    let qual_name = QualName::new(None, html5ever::ns!(html), LocalName::from("style"));

    #[allow(clippy::default_trait_access)]
    let element_data = ElementData {
        name: qual_name,
        attributes: RefCell::new(Attributes {
            map: Default::default(),
        }),
        template_contents: None,
    };

    element_data
        .attributes
        .borrow_mut()
        .insert("style", "text/css".to_owned());

    let style_node = NodeRef::new(NodeData::Element(element_data));
    let text_node = NodeRef::new(NodeData::Text(RefCell::new(style_text.to_owned())));

    style_node.append(text_node);

    element.as_node().append(style_node);
}

#[allow(clippy::missing_panics_doc)] // The select is well formed.
/// This function overrides all `rel` attributes in `<a>` tags to be [noreferrer.](https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes/rel/noreferrer)
///
/// See [this article](https://mathiasbynens.github.io/rel-noopener/) to see how the lack of it could be abused
pub fn add_noreferrer(document: NodeRef) {
    let exp_name = ExpandedName::new(html5ever::namespace_url!(""), "rel");
    let attr = Attribute {
        prefix: None,
        value: "noreferrer".to_string(),
    };

    let anchors = document.select("a").unwrap();

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
        if word.starts_with("http") {
            if let Ok(url) = Url::parse(word) {
                let url: String = strip_from_url(&url).0.into();
                rep.push_str(&format!(r#"<a href="{url}" rel="noreferrer">{url}</a>"#));
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
