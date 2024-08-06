#[cfg(test)]
#[path = "tests/transforms.rs"]
mod tests;

use html5ever::tendril::TendrilSink;
use kuchikiki::{Attribute, ExpandedName, NodeRef};

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
    let style = kuchikiki::parse_html().one(style);

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
