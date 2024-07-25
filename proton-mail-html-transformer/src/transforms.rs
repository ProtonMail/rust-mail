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

#[cfg(test)]
mod test {
    use crate::Transformer;

    #[test]
    fn name() {
        let html = include_str!("../tests/htmls/empty.html");
        let html = Transformer::new(html).inject_style().unwrap().to_string();
        insta::assert_snapshot!(html);
    }
}
