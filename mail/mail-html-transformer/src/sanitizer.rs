#[cfg(test)]
#[path = "tests/sanitizer.rs"]
mod tests;

use crate::css_parser::{parse_style_attribute, parse_stylesheet};
use html5ever::ns;
use html5ever::{LocalName, namespace_url};
use kuchikiki::{Attribute, ExpandedName, NodeData, NodeRef, iter::NodeEdge};
use lightningcss::printer::PrinterOptions;
use lightningcss::properties::custom::{Function, Token, TokenOrValue, Variable};
use lightningcss::values::image::Image;
use lightningcss::values::url::Url;
use lightningcss::visitor::{Visit, VisitTypes, Visitor};
use std::convert::Infallible;
use std::sync::OnceLock;
use std::{collections::HashSet, sync::LazyLock};
use tracing::warn;
use velcro::hash_set;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripStyleSheets {
    Yes,
    No,
}

static STYLE_ATTRIBUTES_SET: LazyLock<HashSet<ExpandedName>> = LazyLock::new(|| {
    hash_set! {
        crate::utils::attribute_name("style"),
        crate::utils::attribute_name("data-proton-original-style"),
        crate::utils::attribute_name("bgcolor"),
        crate::utils::attribute_name("color"),
        crate::utils::attribute_name("background"),
        crate::utils::attribute_name("align"),
        crate::utils::attribute_name("valign"),
        crate::utils::attribute_name("border"),
        crate::utils::attribute_name("cellpadding"),
        crate::utils::attribute_name("cellspacing"),
        crate::utils::attribute_name("width"),
        crate::utils::attribute_name("height"),
        crate::utils::attribute_name("size"),
        crate::utils::attribute_name("face"),
        crate::utils::attribute_name("clear"),
        // Some pasted HTML (e.g., from Wikipedia) includes `srcset` attributes
        // with scheme-relative URLs such as `//upload.wikimedia.org/...`., causing images
        // to fail loading and appear as empty frames.
        crate::utils::attribute_name("srcset"),
    }
});

static TAG_SET: LazyLock<HashSet<LocalName>> = LazyLock::new(|| {
    hash_set! {
        LocalName::from("a"),
        LocalName::from("abbr"),
        LocalName::from("acronym"),
        LocalName::from("address"),
        LocalName::from("area"),
        LocalName::from("article"),
        LocalName::from("aside"),
        LocalName::from("audio"),
        LocalName::from("b"),
        LocalName::from("base"),
        LocalName::from("bdi"),
        LocalName::from("bdo"),
        LocalName::from("big"),
        LocalName::from("blink"),
        LocalName::from("blockquote"),
        LocalName::from("body"),
        LocalName::from("br"),
        LocalName::from("button"),
        LocalName::from("canvas"),
        LocalName::from("caption"),
        LocalName::from("center"),
        LocalName::from("cite"),
        LocalName::from("code"),
        LocalName::from("col"),
        LocalName::from("colgroup"),
        LocalName::from("content"),
        LocalName::from("data"),
        LocalName::from("datalist"),
        LocalName::from("dd"),
        LocalName::from("decorator"),
        LocalName::from("del"),
        LocalName::from("details"),
        LocalName::from("dfn"),
        LocalName::from("dialog"),
        LocalName::from("dir"),
        LocalName::from("div"),
        LocalName::from("dl"),
        LocalName::from("dt"),
        LocalName::from("element"),
        LocalName::from("em"),
        LocalName::from("fieldset"),
        LocalName::from("figcaption"),
        LocalName::from("figure"),
        LocalName::from("font"),
        LocalName::from("footer"),
        LocalName::from("h1"),
        LocalName::from("h2"),
        LocalName::from("h3"),
        LocalName::from("h4"),
        LocalName::from("h5"),
        LocalName::from("h6"),
        LocalName::from("head"),
        LocalName::from("header"),
        LocalName::from("hgroup"),
        LocalName::from("hr"),
        LocalName::from("html"),
        LocalName::from("i"),
        LocalName::from("img"),
        LocalName::from("ins"),
        LocalName::from("kbd"),
        LocalName::from("label"),
        LocalName::from("legend"),
        LocalName::from("li"),
        LocalName::from("main"),
        LocalName::from("map"),
        LocalName::from("mark"),
        LocalName::from("marquee"),
        LocalName::from("menu"),
        LocalName::from("menuitem"),
        LocalName::from("meter"),
        LocalName::from("nav"),
        LocalName::from("nobr"),
        LocalName::from("ol"),
        LocalName::from("optgroup"),
        LocalName::from("option"),
        LocalName::from("output"),
        LocalName::from("p"),
        LocalName::from("picture"),
        LocalName::from("pre"),
        LocalName::from("progress"),
        LocalName::from("proton-src"),
        LocalName::from("q"),
        LocalName::from("rp"),
        LocalName::from("rt"),
        LocalName::from("ruby"),
        LocalName::from("s"),
        LocalName::from("samp"),
        LocalName::from("section"),
        LocalName::from("select"),
        LocalName::from("shadow"),
        LocalName::from("small"),
        LocalName::from("source"),
        LocalName::from("spacer"),
        LocalName::from("span"),
        LocalName::from("strike"),
        LocalName::from("strong"),
        LocalName::from("style"),
        LocalName::from("sub"),
        LocalName::from("summary"),
        LocalName::from("sup"),
        LocalName::from("table"),
        LocalName::from("tbody"),
        LocalName::from("title"),
        LocalName::from("td"),
        LocalName::from("template"),
        LocalName::from("textarea"),
        LocalName::from("tfoot"),
        LocalName::from("th"),
        LocalName::from("thead"),
        LocalName::from("time"),
        LocalName::from("tr"),
        LocalName::from("track"),
        LocalName::from("tt"),
        LocalName::from("u"),
        LocalName::from("ul"),
        LocalName::from("var"),
        LocalName::from("video"),
        LocalName::from("wbr"),
    }
});

static ATTR_SET: LazyLock<HashSet<ExpandedName>> = LazyLock::new(|| {
    hash_set! {
        crate::utils::attribute_name("data-proton-original-style"), // For reverting dark mode injection in inline attributes.
        crate::utils::attribute_name("proton-src"),
        crate::utils::attribute_name("target"),
        crate::utils::attribute_name("accept"),
        crate::utils::attribute_name("action"),
        crate::utils::attribute_name("align"),
        crate::utils::attribute_name("alt"),
        crate::utils::attribute_name("autocapitalize"),
        crate::utils::attribute_name("autocomplete"),
        crate::utils::attribute_name("autopictureinpicture"),
        crate::utils::attribute_name("autoplay"),
        crate::utils::attribute_name("background"),
        crate::utils::attribute_name("bgcolor"),
        crate::utils::attribute_name("border"),
        crate::utils::attribute_name("capture"),
        crate::utils::attribute_name("cellpadding"),
        crate::utils::attribute_name("cellspacing"),
        crate::utils::attribute_name("checked"),
        crate::utils::attribute_name("cite"),
        crate::utils::attribute_name("class"),
        crate::utils::attribute_name("clear"),
        crate::utils::attribute_name("color"),
        crate::utils::attribute_name("cols"),
        crate::utils::attribute_name("colspan"),
        crate::utils::attribute_name("controls"),
        crate::utils::attribute_name("controlslist"),
        crate::utils::attribute_name("coords"),
        crate::utils::attribute_name("crossorigin"),
        crate::utils::attribute_name("datetime"),
        crate::utils::attribute_name("decoding"),
        crate::utils::attribute_name("default"),
        crate::utils::attribute_name("dir"),
        crate::utils::attribute_name("disabled"),
        crate::utils::attribute_name("disablepictureinpicture"),
        crate::utils::attribute_name("disableremoteplayback"),
        crate::utils::attribute_name("download"),
        crate::utils::attribute_name("draggable"),
        crate::utils::attribute_name("enctype"),
        crate::utils::attribute_name("enterkeyhint"),
        crate::utils::attribute_name("face"),
        crate::utils::attribute_name("headers"),
        crate::utils::attribute_name("height"),
        crate::utils::attribute_name("hidden"),
        crate::utils::attribute_name("high"),
        crate::utils::attribute_name("href"),
        crate::utils::attribute_name("hreflang"),
        crate::utils::attribute_name("id"),
        crate::utils::attribute_name("inputmode"),
        crate::utils::attribute_name("integrity"),
        crate::utils::attribute_name("ismap"),
        crate::utils::attribute_name("kind"),
        crate::utils::attribute_name("label"),
        crate::utils::attribute_name("lang"),
        crate::utils::attribute_name("list"),
        crate::utils::attribute_name("loading"),
        crate::utils::attribute_name("loop"),
        crate::utils::attribute_name("low"),
        crate::utils::attribute_name("max"),
        crate::utils::attribute_name("maxlength"),
        crate::utils::attribute_name("media"),
        crate::utils::attribute_name("method"),
        crate::utils::attribute_name("min"),
        crate::utils::attribute_name("minlength"),
        crate::utils::attribute_name("multiple"),
        crate::utils::attribute_name("muted"),
        crate::utils::attribute_name("name"),
        crate::utils::attribute_name("nonce"),
        crate::utils::attribute_name("noshade"),
        crate::utils::attribute_name("novalidate"),
        crate::utils::attribute_name("nowrap"),
        crate::utils::attribute_name("open"),
        crate::utils::attribute_name("optimum"),
        crate::utils::attribute_name("pattern"),
        crate::utils::attribute_name("placeholder"),
        crate::utils::attribute_name("playsinline"),
        crate::utils::attribute_name("popover"),
        crate::utils::attribute_name("popovertarget"),
        crate::utils::attribute_name("popovertargetaction"),
        crate::utils::attribute_name("poster"),
        crate::utils::attribute_name("preload"),
        crate::utils::attribute_name("pubdate"),
        crate::utils::attribute_name("radiogroup"),
        crate::utils::attribute_name("readonly"),
        crate::utils::attribute_name("rel"),
        crate::utils::attribute_name("required"),
        crate::utils::attribute_name("rev"),
        crate::utils::attribute_name("reversed"),
        crate::utils::attribute_name("role"),
        crate::utils::attribute_name("rows"),
        crate::utils::attribute_name("rowspan"),
        crate::utils::attribute_name("spellcheck"),
        crate::utils::attribute_name("scope"),
        crate::utils::attribute_name("selected"),
        crate::utils::attribute_name("shape"),
        crate::utils::attribute_name("size"),
        crate::utils::attribute_name("sizes"),
        crate::utils::attribute_name("span"),
        crate::utils::attribute_name("srclang"),
        crate::utils::attribute_name("start"),
        crate::utils::attribute_name("src"),
        crate::utils::attribute_name("step"),
        crate::utils::attribute_name("style"),
        crate::utils::attribute_name("summary"),
        crate::utils::attribute_name("tabindex"),
        crate::utils::attribute_name("title"),
        crate::utils::attribute_name("translate"),
        crate::utils::attribute_name("type"),
        crate::utils::attribute_name("usemap"),
        crate::utils::attribute_name("valign"),
        crate::utils::attribute_name("value"),
        crate::utils::attribute_name("width"),
        crate::utils::attribute_name("wrap"),
        crate::utils::attribute_name("xmlns"),
        crate::utils::attribute_name("slot"),
    }
});

/// Tags that should be removed with their inner HTML.
static TAGS_TO_REMOVE_WITH_INNER_HTML: LazyLock<HashSet<LocalName>> = LazyLock::new(|| {
    hash_set! {
        LocalName::from("script"),
    }
});

#[must_use]
/// This function removes the tags and attributes defined in this file
///
/// Such a whitelist come from the JS library [DOMPurify](https://github.com/cure53/DOMPurify) with a few exceptions:
/// - Extra allowed tags: `<proton-src />`, `<base />`
/// - Extra allowed attributes: `proton-src`, `target`
/// - Extra disallowed tags: `style`, `input`, `form`
/// - Extra disallowed attributes `srcset`, `for`
/// - Only html tags and attributes are included. This is, svg and mathML are disallowed.
pub fn strip_whitelist(doc: NodeRef, strip_style_sheets: StripStyleSheets) -> u64 {
    let css_style_attribute = crate::utils::attribute_name("style");
    let css_style_node = LocalName::from("style");

    let rem = doc
        .traverse_inclusive()
        .filter_map(|node| match node {
            NodeEdge::Start(node_ref) => Some(node_ref),
            NodeEdge::End(_) => None,
        })
        .filter_map(|node_ref| match node_ref.data() {
            NodeData::Element(e) => {
                let tag_name = &e.name.local;
                if !TAG_SET.contains(tag_name) {
                    let should_remove_inner_html =
                        TAGS_TO_REMOVE_WITH_INNER_HTML.contains(tag_name);
                    return Some((node_ref, should_remove_inner_html));
                }

                // Remove style elements when sanitizing pasted content
                if e.name.local == css_style_node {
                    if strip_style_sheets == StripStyleSheets::Yes {
                        return Some((node_ref, true));
                    }
                    // sanitize style sheet urls - invalid urls are stripped by the parser.
                    node_ref.children().for_each(|child| {
                        if let NodeData::Text(text) = child.data() {
                            handle_style_sheet(&mut text.borrow_mut());
                        }
                    });
                }

                let mut attrs = e.attributes.borrow_mut();
                attrs.map.retain(|name, value| {
                    // Remove style-related attributes when sanitizing pasted content
                    if strip_style_sheets == StripStyleSheets::Yes
                        && STYLE_ATTRIBUTES_SET.contains(name)
                    {
                        return false;
                    }

                    if !(ATTR_SET.contains(name) && validate_uri_attribute(name, value)) {
                        return false;
                    }

                    // sanitize css style attributes urls - invalid urls are stripped
                    // by the parser.
                    if name == &css_style_attribute {
                        handle_style_attribute(&mut value.value);
                    }

                    true
                });
                None
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    let total = rem.len();
    for (node, should_remove_inner_html) in rem {
        if !should_remove_inner_html {
            for child in node.children() {
                node.insert_before(child);
            }
        }
        node.detach();
    }
    total as u64
}

fn validate_uri_attribute(name: &ExpandedName, value: &mut Attribute) -> bool {
    if !get_uri_attributes().contains(name) {
        return true;
    }

    is_valid_url_for_attribute(&value.value, name)
}

fn is_valid_url(value: &str) -> bool {
    let Ok(uri) = url::Url::parse(value) else {
        // Invalid urls should be ignored
        return false;
    };

    is_valid_url_type(uri)
}

fn is_valid_url_for_attribute(value: &str, attribute_name: &ExpandedName) -> bool {
    let Ok(uri) = url::Url::parse(value) else {
        // Invalid urls should be ignored
        return false;
    };

    is_valid_url_type_for_attribute(uri, attribute_name)
}

// Check if the cid data is actually valid
// https://datatracker.ietf.org/doc/html/rfc2392
fn is_valid_cid(uri: &url::Url) -> bool {
    let cid_data = uri.path();
    if email_address::EmailAddress::parse_with_options(
        cid_data,
        email_address::Options::default()
            .without_display_text()
            .with_long_local_parts()
            .with_required_tld(),
    )
    .is_err()
    {
        // We are using uids for while in ET, check if this actually a UID or a non valid
        // email address that may contain email chars for greater compatibility
        uri.path()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '@')
    } else {
        true
    }
}

fn is_valid_url_type(uri: url::Url) -> bool {
    let scheme = uri.scheme().to_lowercase();

    if scheme == "cid" {
        return is_valid_cid(&uri);
    }

    scheme == "https" || scheme == "http" || scheme == "data"
}

fn is_valid_url_type_for_attribute(uri: url::Url, attribute_name: &ExpandedName) -> bool {
    let scheme = uri.scheme().to_lowercase();

    if scheme == "cid" {
        return is_valid_cid(&uri);
    }

    if scheme == "mailto" {
        return MAILTO_COMPATIBLE_ATTRIBUTES.contains(attribute_name);
    }

    scheme == "https" || scheme == "http" || scheme == "data"
}

static URI_ATTRIBUTES: OnceLock<HashSet<ExpandedName>> = OnceLock::new();

static MAILTO_COMPATIBLE_ATTRIBUTES: LazyLock<HashSet<ExpandedName>> = LazyLock::new(|| {
    hash_set! {
        crate::utils::attribute_name("href"),
        crate::utils::attribute_name_ex(ns!(xlink), "href"),
    }
});

fn get_uri_attributes() -> &'static HashSet<ExpandedName> {
    URI_ATTRIBUTES.get_or_init(|| {
        HashSet::from([
            crate::utils::attribute_name("url"),
            crate::utils::attribute_name("src"),
            crate::utils::attribute_name("srcset"),
            crate::utils::attribute_name("svg"),
            crate::utils::attribute_name("background"),
            crate::utils::attribute_name("poster"),
            crate::utils::attribute_name("data-src"),
            crate::utils::attribute_name("href"),
            crate::utils::attribute_name("cite"),
            crate::utils::attribute_name("action"),
            crate::utils::attribute_name("profile"),
            crate::utils::attribute_name("longdesc"),
            crate::utils::attribute_name("classid"),
            crate::utils::attribute_name("codebase"),
            crate::utils::attribute_name("data"),
            crate::utils::attribute_name("usemap"),
            crate::utils::attribute_name("formaction"),
            crate::utils::attribute_name("poster"),
            crate::utils::attribute_name("archive"),
            crate::utils::attribute_name_ex(ns!(xlink), "href"),
        ])
    })
}

fn handle_style_sheet(css: &mut String) {
    let Ok(mut sheet) = parse_stylesheet(css).inspect_err(|e| {
        warn!("StyleSheet parsing failed: {}", e);
    }) else {
        return;
    };

    let mut visitor = CssUrlVisitor;

    let _ = sheet.visit(&mut visitor);

    let Ok(patched) = sheet.to_css(PrinterOptions::default()) else {
        warn!("Failed to convert style sheet to css value");
        return;
    };

    drop(sheet);

    *css = patched.code;
}

fn handle_style_attribute(css: &mut String) {
    let Ok(mut style_attribute) = parse_style_attribute(css).inspect_err(|e| {
        warn!("Style attribute parsing failed: {}", e);
    }) else {
        return;
    };

    let mut visitor = CssUrlVisitor;

    let _ = style_attribute.visit(&mut visitor);

    let Ok(patched) = style_attribute.to_css(PrinterOptions::default()) else {
        warn!("Failed to convert style attribute to css value");
        return;
    };

    drop(style_attribute);

    *css = patched.code;
}

#[derive(Default)]
struct CssUrlVisitor;
impl<'i> Visitor<'i> for CssUrlVisitor {
    type Error = Infallible;

    fn visit_types(&self) -> VisitTypes {
        VisitTypes::all()
    }

    fn visit_url(&mut self, url: &mut Url<'i>) -> Result<(), Self::Error> {
        if !is_valid_url(&url.url) {
            url.url = String::new().into();
        }
        url.visit_children(self)
    }

    fn visit_image(&mut self, image: &mut Image<'i>) -> Result<(), Self::Error> {
        match image {
            Image::None | Image::Gradient(_) => Ok(()),
            Image::Url(url) => self.visit_url(url),
            Image::ImageSet(set) => {
                for option in &mut set.options {
                    self.visit_image(&mut option.image)?;
                }
                Ok(())
            }
        }
    }

    fn visit_variable(&mut self, var: &mut Variable<'i>) -> Result<(), Self::Error> {
        if let Some(tokens) = &mut var.fallback {
            self.visit_token_list(tokens)?;
        }
        var.visit_children(self)
    }

    fn visit_function(&mut self, function: &mut Function<'i>) -> Result<(), Self::Error> {
        if function.name.to_lowercase() == "image-set" {
            self.visit_token_list(&mut function.arguments)
        } else {
            function.visit_children(self)
        }
    }

    fn visit_token(&mut self, token: &mut TokenOrValue<'i>) -> Result<(), Self::Error> {
        if let TokenOrValue::Token(token) = token {
            match token {
                Token::String(value) => {
                    // This string could be anything, we can't make any assumptions, but
                    // if it happens to be an uri, we can at least check it.
                    if let Ok(uri) = url::Url::parse(value)
                        && !is_valid_url_type(uri)
                    {
                        *value = String::new().into();
                    }
                }
                Token::UnquotedUrl(url) => {
                    if !is_valid_url(url) {
                        *url = String::new().into();
                    }
                }
                _ => {}
            }
        }

        token.visit_children(self)
    }
}
