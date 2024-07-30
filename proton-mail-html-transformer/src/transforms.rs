use html5ever::{namespace_url, tendril::TendrilSink, LocalName, QualName};
use kuchikiki::{iter::NodeEdge, Attribute, ExpandedName, NodeData, NodeRef};

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
    let exp_name = ExpandedName::new(html5ever::namespace_url!(""), "ref");
    let attr = Attribute {
        prefix: None,
        value: "noreferrer".to_string(),
    };

    let anchors = document.select("a").unwrap();

    for anchor in anchors {
        let mut attrs = anchor.attributes.borrow_mut();
        attrs
            .map
            .entry(exp_name.clone())
            .or_insert_with(|| attr.clone());
    }
}

#[cfg(test)]
mod test {
    use crate::Transformer;

    #[test]
    fn inject_style() {
        let html = include_str!("../tests/htmls/empty.html");
        let html = Transformer::new(html).inject_style().to_string();
        insta::assert_snapshot!(html);
    }

    #[test]
    fn inject_style_fail() {
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
}
