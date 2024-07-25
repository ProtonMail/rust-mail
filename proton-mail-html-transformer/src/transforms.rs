use html5ever::tendril::TendrilSink;
use kuchikiki::{Attribute, ExpandedName, NodeRef};

use crate::Error;

#[allow(clippy::needless_pass_by_value)]
pub fn inject_style(document: NodeRef) -> Result<(), Error> {
    let element = document
        .select_first("head")
        .map_err(|()| Error::HeadElementNotFound)?;

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
    Ok(())
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
        let html = Transformer::new(html).inject_style().unwrap().to_string();
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
