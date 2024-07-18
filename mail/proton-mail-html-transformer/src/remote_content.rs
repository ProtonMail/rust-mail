//! This pass focuses on blocking remote content from loading and/or patching remote content Urls to
//! go through the Proton Proxy.
//!
//! Since these are use configurable options, each of these has a separate pass which undoes the
//! changes.

use html5ever::{namespace_url, ns, Namespace};
use kuchikiki::iter::NodeEdge;
use kuchikiki::{Attributes, ExpandedName, NodeRef};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid Selector: {0}")]
    Selector(String),
    #[error("Url: {0}")]
    Url(#[from] url::ParseError),
}

const WHITELISTED_ELEMENTS: [&str; 3] = ["a", "base", "area"];

const PROTON_PREFIX: &str = "proton-";

/// Disable all remote content by prefixing known attributes with `proton-`.
///
/// To reverse this pass, see [`undo_disable_remote_content()`].
///
/// # Example
///
/// This will convert:
///
/// ``` html
/// <img src="...">
/// ```
/// Into:
///
/// ``` html
/// <img proton-src="...">
/// ```
///
/// # Errors
///
/// Returns an error if the selector failed to build.
pub fn disable_remote_content(document: &NodeRef) -> Result<(), Error> {
    // Unfortunately the selector library does not allow use to query attributes that are not part
    // of the html standard. Attributes such as 'xlink:href` need to handled manually, so
    // we need to traverse the document manually and check each attribute ourselves.
    let attribute_list = AttributeInfo::default_list();

    for_each_element(document, move |attributes| {
        for item in &attribute_list {
            let Some(attribute) = attributes.map.remove(&item.enabled) else {
                continue;
            };

            attributes.map.insert(item.disabled.clone(), attribute);
        }
        Ok(())
    })
}

/// Re-enables all disabled content by stripping the `proton-` prefix.
///
/// This pass does the opposite of [`disable_remote_content()`].
///
/// # Example
///
/// This will convert:
///
/// ``` html
/// <img proton-src="...">
/// ```
/// Into:
///
/// ``` html
/// <img src="...">
/// ```
///
/// # Errors
///
/// Returns an error if the selector failed to build.
pub fn undo_disable_remote_content(document: &NodeRef) -> Result<(), Error> {
    // Unfortunately the selector library does not allow use to query attributes that are not part
    // of the html standard. Attributes such as 'xlink:href` need to handled manually, so
    // we need to traverse the document manually and check each attribute ourselves.
    let attribute_list = AttributeInfo::default_list();

    for_each_element(document, move |attributes| {
        for item in &attribute_list {
            let Some(attribute) = attributes.map.remove(&item.disabled) else {
                continue;
            };

            attributes.map.insert(item.enabled.clone(), attribute);
        }
        Ok(())
    })
}

/// Iterate over the `document` and apply the `closure` to each element's attributes.
fn for_each_element(
    document: &NodeRef,
    closure: impl Fn(&mut Attributes) -> Result<(), Error>,
) -> Result<(), Error> {
    for node in document.traverse_inclusive() {
        let NodeEdge::Start(node_ref) = node else {
            continue;
        };

        let Some(element) = node_ref.as_element() else {
            continue;
        };

        if WHITELISTED_ELEMENTS.contains(&element.name.local.as_ref()) {
            continue;
        }

        let mut attributes = element.attributes.borrow_mut();

        (closure)(&mut attributes)?;
    }
    Ok(())
}

/// Details on how the attributes should be represented when enabled or disabled.
struct AttributeInfo {
    /// Value of the attribute if it is enabled.
    enabled: ExpandedName,
    /// Value of the attribute if it is disabled.
    disabled: ExpandedName,
}

impl AttributeInfo {
    /// Generate a new instance with `namespace` and `value`.
    pub fn new(namespace: Namespace, value: &str) -> Self {
        Self {
            enabled: ExpandedName::new(namespace.clone(), value),
            disabled: ExpandedName::new(namespace, format! {"{PROTON_PREFIX}{value}"}),
        }
    }

    /// Generate a custom tailored replacement for `xlink:href` attributes.
    ///
    /// This need to be handled differently since the parser does not recognize the patched
    /// version as being a member of the `xlink` namespace.
    pub fn xlink_href() -> Self {
        Self {
            enabled: ExpandedName::new(ns!(xlink), "href"),
            disabled: ExpandedName::new(ns!(), format! {"xlink:{PROTON_PREFIX}href"}),
        }
    }

    /// Default list of attributes we need to patch.
    fn default_list() -> Vec<AttributeInfo> {
        vec![
            AttributeInfo::new(ns!(), "url"),
            AttributeInfo::xlink_href(),
            AttributeInfo::new(ns!(), "src"),
            AttributeInfo::new(ns!(), "srcset"),
            AttributeInfo::new(ns!(), "svg"),
            AttributeInfo::new(ns!(), "background"),
            AttributeInfo::new(ns!(), "poster"),
            AttributeInfo::new(ns!(), "data-src"),
            AttributeInfo::new(ns!(), "href"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::Transformer;
    use html5ever::tendril::TendrilSink;

    // Note: If you need more test cases, it is recommended to set the transformed attribute
    // at the end of the element since that is where it will be inserted after transformation.

    const TEST_DOCUMENT: &str = r##"
<section>
    <svg id="svigi" width="5cm" height="4cm" version="1.1"
    xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
        <image x="0" y="0" height="50px" width="50px" xlink:href="firefox.jpg" />
        <image x="0" y="0" height="50px" width="50px" xlink:href="chrome.jpg" />
        <image x="0" y="0" height="50px" width="50px" href="svg-href.jpg" />
    </svg>
    <div>
        <img border="0" usemap="#fp" src="cats.jpg ">
        <map name="fp">
            <area coords="0,0,800,800" href="proton_exploit.html" shape="rect" target="_blank" >
        </map>
    </div>

    <img width="" height="" alt="" src="mon-image.jpg" srcset="mon-imageHD.jpg 2x">
    <img width="" height="" alt="" src="lol-image.jpg" srcset="lol-imageHD.jpg 2x">
    <img width="" height="" alt="" data-src="lol-image.jpg">
    <a href="lol-image.jpg">Alll</a>
    <a href="jeanne-image.jpg">Alll</a>
    <div background="jeanne-image.jpg">Alll</div>
    <div background="jeanne-image2.jpg">Alll</div>
    <p style="font-size:10.0pt;font-family:\\2018Calibri\\2019;color:black">
        Example style that caused regexps to crash
    </p>
    <img id="babase64" src="data:image/jpg;base64,iVBORw0KGgoAAAANSUhEUgAABoIAAAVSCAYAAAAisOk2AAAMS2lDQ1BJQ0MgUHJv
    ZmlsZQAASImVVwdYU8kWnltSSWiBUKSE3kQp0qWE0CIISBVshCSQUGJMCCJ2FlkF
    1y4ioK7oqoiLrgWQtaKudVHs/aGIysq6WLCh8iYF1tXvvfe9831z758z5/ynZO69
    MwDo1PKk0jxUF4B8SYEsITKUNTEtnUXqAgSgD1AwGozk8eVSdnx8DIAydP+nvLkO"
    />
</section>
"##;

    const TEST_DOCUMENT_REMOTE_CONTENT_DISABLED: &str = r##"
<section>
    <svg id="svigi" width="5cm" height="4cm" version="1.1"
    xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
        <image x="0" y="0" height="50px" width="50px" xlink:proton-href="firefox.jpg" />
        <image x="0" y="0" height="50px" width="50px" xlink:proton-href="chrome.jpg" />
        <image x="0" y="0" height="50px" width="50px" proton-href="svg-href.jpg" />
    </svg>
    <div>
        <img border="0" usemap="#fp" proton-src="cats.jpg ">
        <map name="fp">
            <area coords="0,0,800,800" href="proton_exploit.html" shape="rect" target="_blank" >
        </map>
    </div>

    <img width="" height="" alt="" proton-src="mon-image.jpg" proton-srcset="mon-imageHD.jpg 2x">
    <img width="" height="" alt="" proton-src="lol-image.jpg" proton-srcset="lol-imageHD.jpg 2x">
    <img width="" height="" alt="" proton-data-src="lol-image.jpg">
    <a href="lol-image.jpg">Alll</a>
    <a href="jeanne-image.jpg">Alll</a>
    <div proton-background="jeanne-image.jpg">Alll</div>
    <div proton-background="jeanne-image2.jpg">Alll</div>
    <p style="font-size:10.0pt;font-family:\\2018Calibri\\2019;color:black">
        Example style that caused regexps to crash
    </p>
    <img id="babase64" proton-src="data:image/jpg;base64,iVBORw0KGgoAAAANSUhEUgAABoIAAAVSCAYAAAAisOk2AAAMS2lDQ1BJQ0MgUHJv
    ZmlsZQAASImVVwdYU8kWnltSSWiBUKSE3kQp0qWE0CIISBVshCSQUGJMCCJ2FlkF
    1y4ioK7oqoiLrgWQtaKudVHs/aGIysq6WLCh8iYF1tXvvfe9831z758z5/ynZO69
    MwDo1PKk0jxUF4B8SYEsITKUNTEtnUXqAgSgD1AwGozk8eVSdnx8DIAydP+nvLkO"
    />
</section>
"##;

    #[test]
    fn disable_remote_elements() {
        let mut transformer = Transformer::new(TEST_DOCUMENT);
        transformer.disable_remote_content().unwrap();
        let output = transformer.to_string();

        let expected = kuchikiki::parse_html().one(TEST_DOCUMENT_REMOTE_CONTENT_DISABLED);
        assert_eq!(expected.to_string(), output.to_string());
    }

    #[test]
    fn enable_remote_elements() {
        let mut transformer = Transformer::new(TEST_DOCUMENT_REMOTE_CONTENT_DISABLED);
        transformer.enable_remote_content().unwrap();
        let output = transformer.to_string();

        let expected = kuchikiki::parse_html().one(TEST_DOCUMENT);
        assert_eq!(expected.to_string(), output.to_string());
    }

    #[test]
    fn disable_enable_remote_elements_cycle() {
        let mut transformer = Transformer::new(TEST_DOCUMENT);
        transformer.disable_remote_content().unwrap();
        transformer.enable_remote_content().unwrap();
        let output = transformer.to_string();

        let expected = kuchikiki::parse_html().one(TEST_DOCUMENT);
        assert_eq!(expected.to_string(), output.to_string());
    }
}
