use html5ever::{LocalName, QualName, namespace_url, ns};
use kuchikiki::NodeRef;
use kuchikiki::traits::TendrilSink;

#[derive(Debug, thiserror::Error)]
#[error("Invalid selector")]
pub struct InvalidSelectorError;

/// Replace the inner content of the first div with `div_class` with new html `content`.
///
/// If no such div can be found, this method does nothing. If multiple divs are present only
/// the first div's content is replaced.
pub fn replace_inner_div(
    document: &NodeRef,
    div_class: &str,
    content: &str,
) -> Result<(), InvalidSelectorError> {
    let replacement = kuchikiki::parse_fragment(
        QualName::new(None, ns!(html), LocalName::from("div_replace")),
        vec![],
    )
    .one(content);

    let selector = format!("div.{div_class}");

    let mut nodes = document
        .select(&selector)
        .map_err(|()| InvalidSelectorError)?;

    let Some(node) = nodes.next() else {
        return Ok(());
    };
    let node = node.as_node();
    for child in node.children() {
        child.detach();
    }

    // Parsing the fragment seems to add the html tag and there is no option to remove
    // it so we have to this manually.
    let html_node = replacement.select_first("html").expect("Should never fail");
    for replacement in html_node.as_node().children() {
        replacement.detach();
        node.append(replacement);
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    const INPUT: &str = r#"
<html>
<body>
<div class="foo_bar">
    <a href="hello.com"> hello</a>
</div>
</body>
</html>
"#;

    const INPUT_MULTI: &str = r#"
<html>
<body>
<div class="foo_bar">
    <a href="hello.com"> hello</a>
</div>
<div class="foo_bar">
    <a href="hello.com"> I remain unchagned</a>
</div>
</body>
</html>
"#;

    const INPUT_WITHOUT_MATCHES: &str = r#"
<html>
<body>
<div class="not_foo_bar">
    <a href="hello.com"> hello</a>
</div>
</body>
</html>
"#;

    const REPLACEMENT: &str = r"
<ul>
<li>hello</li>
<li>world</li>
</ul>
";

    #[test]
    fn replace_single_div() {
        let main = kuchikiki::parse_html().one(INPUT);
        replace_inner_div(&main, "foo_bar", REPLACEMENT).unwrap();
        insta::assert_snapshot!(main.to_string());
    }

    #[test]
    fn replace_multiple_div() {
        let main = kuchikiki::parse_html().one(INPUT_MULTI);
        replace_inner_div(&main, "foo_bar", REPLACEMENT).unwrap();
        insta::assert_snapshot!(main.to_string());
    }

    #[test]
    fn unable_to_match() {
        let main = kuchikiki::parse_html().one(INPUT_WITHOUT_MATCHES);
        replace_inner_div(&main, "foo_bar", REPLACEMENT).unwrap();
        insta::assert_snapshot!(main.to_string());
    }

    #[test]
    fn empty_replacement() {
        let main = kuchikiki::parse_html().one(INPUT);
        replace_inner_div(&main, "foo_bar", "").unwrap();
        insta::assert_snapshot!(main.to_string());
    }
}
