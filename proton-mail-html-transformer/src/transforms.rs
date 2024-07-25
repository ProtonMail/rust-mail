use html5ever::tendril::TendrilSink;
use kuchikiki::NodeRef;

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

#[allow(clippy::needless_pass_by_value)]
pub fn add_noreferrer(document: NodeRef) {
    let anchors = document.select("a").unwrap();

    for anchor in anchors {
        let mut attrs = anchor.attributes.borrow_mut();
        _ = attrs.insert("ref", "noreferrer".to_string());
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
