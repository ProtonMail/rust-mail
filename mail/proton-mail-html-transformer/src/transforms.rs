use html5ever::{namespace_url, tendril::TendrilSink, LocalName, QualName};
use kuchikiki::{iter::NodeEdge, Attribute, ExpandedName, NodeData, NodeRef};
use url::Url;

use crate::utm::strip_from_url;

fn node_ref_from_str(html: &str, tag: &str) -> NodeRef {
    let qual_name = QualName::new(None, html5ever::ns!(html), LocalName::from(tag));
    kuchikiki::parse_fragment(qual_name, vec![]).one(html)
}

#[allow(clippy::missing_panics_doc)]
pub fn inject_style(document: NodeRef) {
    let element = document.select_first("head").unwrap(); // kuckikiki always adds it

    let style = "
<style>
  body {
    background-color: Canvas;
    color: CanvasText;
    color-scheme: light dark;
  }
</style>
";
    let style = node_ref_from_str(style, "head");

    element.as_node().append(style);
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
            if let Ok(url) = url::Url::parse(word) {
                let url: String = strip_from_url(&url).into();
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

#[allow(clippy::missing_panics_doc)] // the select is well formed.
pub fn proxy_images(document: NodeRef, user_session_id: &str) {
    let elements = document.select("img").unwrap();
    let mut base = Url::parse("https://mail.proton.me/api/core/v4/images").unwrap();
    base.query_pairs_mut()
        .append_pair("DryRun", "0")
        .append_pair("UID", user_session_id);

    for element in elements {
        let mut attrs = element.attributes.borrow_mut();

        attrs.entry("src").and_modify(|src| {
            let mut new = base.clone();
            new.query_pairs_mut().append_pair("Url", &src.value); // PERF: This is kinda slow
            src.value = new.into();
        });
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::needless_raw_string_hashes)]
    use crate::Transformer;
    #[test]
    fn inject_style() {
        let html = include_str!("../tests/htmls/empty.html");
        let html = Transformer::new(html).inject_style().to_string();
        insta::assert_snapshot!(html);
    }

    #[test]
    fn inject_style_no_head() {
        let html = r"
        <div>
          ain't no `head` here boss
        </div>
        ";
        let html = Transformer::new(html).inject_style().to_string();
        insta::assert_snapshot!(html);
    }

    #[test]
    fn add_noreferrer() {
        let html = r#"
        <div>
          <a href="proton.me"/>
          <a href="proton.me" rel="foobar"/>
        </div>
        "#;
        let html = Transformer::new(html).add_noreferrer().to_string();
        insta::assert_snapshot!(html);
    }

    #[test]
    fn insert_links() {
        let html = r#"
        <div id="1"> this is some content without a link </div>
        <div id="2">https://proton.me</div>
        <div id="3"> this is some content with a link to https://proton.me :) </div>
        <div id="4"> strippin' balls https://ads.com?utm_source=tracker </div>
        <div id="5"> incompete url not handled: proton.me </div>
        <div id="6"> empty url not matched: https: </div>
        <div id="7"> empty url not matched: mailto: </div>
        <div id="8"> localhost http://localhost </div>
        <div id="9"> ip http://127.0.0.1 </div>
        <div id="10"> mailto:foo@bar </div>
        "#;
        let html = Transformer::new(html).insert_links().to_string();
        insta::assert_snapshot!(html);
    }

    #[test]
    fn proxy_images() {
        let html = r#"
        <body>
        <img id="1" src="bad url">
        <img id="2" src="https://ads.com">
        <img id="2" src="https://ads.com?utm_source=tracker">
        </body>
        "#;
        let html = Transformer::new(html)
            .proxy_images("MYTOKEN123")
            .to_string();
        insta::assert_snapshot!(html);
    }
}
