#[cfg(test)]
#[path = "tests/sanitizer.rs"]
mod tests;

use html5ever::ns;
use html5ever::{LocalName, namespace_url};
use kuchikiki::{Attribute, ExpandedName, NodeData, NodeRef, iter::NodeEdge};
use lightningcss::printer::PrinterOptions;
use lightningcss::properties::custom::{Function, Token, TokenOrValue, Variable};
use lightningcss::stylesheet::{ParserOptions, StyleAttribute, StyleSheet};
use lightningcss::values::image::Image;
use lightningcss::values::url::Url;
use lightningcss::visitor::{Visit, VisitTypes, Visitor};
use std::convert::Infallible;
use std::sync::OnceLock;
use std::{collections::HashSet, sync::LazyLock};
use tracing::warn;
use velcro::hash_set;

static TAG_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    hash_set! {
        "a",
        "abbr",
        "acronym",
        "address",
        "area",
        "article",
        "aside",
        "audio",
        "b",
        "base",
        "bdi",
        "bdo",
        "big",
        "blink",
        "blockquote",
        "body",
        "br",
        "button",
        "canvas",
        "caption",
        "center",
        "cite",
        "code",
        "col",
        "colgroup",
        "content",
        "data",
        "datalist",
        "dd",
        "decorator",
        "del",
        "details",
        "dfn",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "element",
        "em",
        "fieldset",
        "figcaption",
        "figure",
        "font",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hgroup",
        "hr",
        "html",
        "i",
        "img",
        "ins",
        "kbd",
        "label",
        "legend",
        "li",
        "main",
        "map",
        "mark",
        "marquee",
        "menu",
        "menuitem",
        "meter",
        "nav",
        "nobr",
        "ol",
        "optgroup",
        "option",
        "output",
        "p",
        "picture",
        "pre",
        "progress",
        "proton-src",
        "q",
        "rp",
        "rt",
        "ruby",
        "s",
        "samp",
        "section",
        "select",
        "shadow",
        "small",
        "source",
        "spacer",
        "span",
        "strike",
        "strong",
        "style",
        "sub",
        "summary",
        "sup",
        "table",
        "tbody",
        "title",
        "td",
        "template",
        "textarea",
        "tfoot",
        "th",
        "thead",
        "time",
        "tr",
        "track",
        "tt",
        "u",
        "ul",
        "var",
        "video",
        "wbr",
    }
});

static ATTR_SET: LazyLock<HashSet<LocalName>> = LazyLock::new(|| {
    hash_set! {
        LocalName::from("data-proton-original-style"), // For reverting dark mode injection in inline attributes.
        LocalName::from("proton-src"),
        LocalName::from("target"),
        LocalName::from("accept"),
        LocalName::from("action"),
        LocalName::from("align"),
        LocalName::from("alt"),
        LocalName::from("autocapitalize"),
        LocalName::from("autocomplete"),
        LocalName::from("autopictureinpicture"),
        LocalName::from("autoplay"),
        LocalName::from("background"),
        LocalName::from("bgcolor"),
        LocalName::from("border"),
        LocalName::from("capture"),
        LocalName::from("cellpadding"),
        LocalName::from("cellspacing"),
        LocalName::from("checked"),
        LocalName::from("cite"),
        LocalName::from("class"),
        LocalName::from("clear"),
        LocalName::from("color"),
        LocalName::from("cols"),
        LocalName::from("colspan"),
        LocalName::from("controls"),
        LocalName::from("controlslist"),
        LocalName::from("coords"),
        LocalName::from("crossorigin"),
        LocalName::from("datetime"),
        LocalName::from("decoding"),
        LocalName::from("default"),
        LocalName::from("dir"),
        LocalName::from("disabled"),
        LocalName::from("disablepictureinpicture"),
        LocalName::from("disableremoteplayback"),
        LocalName::from("download"),
        LocalName::from("draggable"),
        LocalName::from("enctype"),
        LocalName::from("enterkeyhint"),
        LocalName::from("face"),
        LocalName::from("headers"),
        LocalName::from("height"),
        LocalName::from("hidden"),
        LocalName::from("high"),
        LocalName::from("href"),
        LocalName::from("hreflang"),
        LocalName::from("id"),
        LocalName::from("inputmode"),
        LocalName::from("integrity"),
        LocalName::from("ismap"),
        LocalName::from("kind"),
        LocalName::from("label"),
        LocalName::from("lang"),
        LocalName::from("list"),
        LocalName::from("loading"),
        LocalName::from("loop"),
        LocalName::from("low"),
        LocalName::from("max"),
        LocalName::from("maxlength"),
        LocalName::from("media"),
        LocalName::from("method"),
        LocalName::from("min"),
        LocalName::from("minlength"),
        LocalName::from("multiple"),
        LocalName::from("muted"),
        LocalName::from("name"),
        LocalName::from("nonce"),
        LocalName::from("noshade"),
        LocalName::from("novalidate"),
        LocalName::from("nowrap"),
        LocalName::from("open"),
        LocalName::from("optimum"),
        LocalName::from("pattern"),
        LocalName::from("placeholder"),
        LocalName::from("playsinline"),
        LocalName::from("popover"),
        LocalName::from("popovertarget"),
        LocalName::from("popovertargetaction"),
        LocalName::from("poster"),
        LocalName::from("preload"),
        LocalName::from("pubdate"),
        LocalName::from("radiogroup"),
        LocalName::from("readonly"),
        LocalName::from("rel"),
        LocalName::from("required"),
        LocalName::from("rev"),
        LocalName::from("reversed"),
        LocalName::from("role"),
        LocalName::from("rows"),
        LocalName::from("rowspan"),
        LocalName::from("spellcheck"),
        LocalName::from("scope"),
        LocalName::from("selected"),
        LocalName::from("shape"),
        LocalName::from("size"),
        LocalName::from("sizes"),
        LocalName::from("span"),
        LocalName::from("srclang"),
        LocalName::from("start"),
        LocalName::from("src"),
        LocalName::from("step"),
        LocalName::from("style"),
        LocalName::from("summary"),
        LocalName::from("tabindex"),
        LocalName::from("title"),
        LocalName::from("translate"),
        LocalName::from("type"),
        LocalName::from("usemap"),
        LocalName::from("valign"),
        LocalName::from("value"),
        LocalName::from("width"),
        LocalName::from("wrap"),
        LocalName::from("xmlns"),
        LocalName::from("slot"),
    }
});

/// Tags that should be removed with their inner HTML.
static TAGS_TO_REMOVE_WITH_INNER_HTML: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    hash_set! {
        "script",
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
pub fn strip_whitelist(doc: NodeRef) -> u64 {
    let css_style_attribute = ExpandedName::new("", "style");
    let rem = doc
        .traverse_inclusive()
        .filter_map(|node| match node {
            NodeEdge::Start(node_ref) => Some(node_ref),
            NodeEdge::End(_) => None,
        })
        .filter_map(|node_ref| match node_ref.data() {
            NodeData::Element(e) => {
                let tag_name: &str = &e.name.local;
                if !TAG_SET.contains(tag_name) {
                    let should_remove_inner_html =
                        TAGS_TO_REMOVE_WITH_INNER_HTML.contains(tag_name);
                    return Some((node_ref, should_remove_inner_html));
                }

                // sanitize style sheet urls - invalid urls are stripped by the parser.
                if e.name.local.as_ref() == "style" {
                    node_ref.children().for_each(|child| {
                        if let NodeData::Text(text) = child.data() {
                            handle_style_sheet(&mut text.borrow_mut());
                        }
                    });
                }

                let mut attrs = e.attributes.borrow_mut();
                attrs.map.retain(|name, value| {
                    if !(ATTR_SET.contains(&name.local) && validate_uri_attribute(name, value)) {
                        return false;
                    }

                    // sanitize css style attributes urls - invalid urls are stripped
                    // by the parser.
                    if *name == css_style_attribute {
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
        ExpandedName::new("", "href"),
        ExpandedName::new(ns!(xlink), "href"),
    }
});

fn get_uri_attributes() -> &'static HashSet<ExpandedName> {
    URI_ATTRIBUTES.get_or_init(|| {
        HashSet::from([
            ExpandedName::new("", "url"),
            ExpandedName::new("", "src"),
            ExpandedName::new("", "srcset"),
            ExpandedName::new("", "svg"),
            ExpandedName::new("", "background"),
            ExpandedName::new("", "poster"),
            ExpandedName::new("", "data-src"),
            ExpandedName::new("", "href"),
            ExpandedName::new("", "cite"),
            ExpandedName::new("", "action"),
            ExpandedName::new("", "profile"),
            ExpandedName::new("", "longdesc"),
            ExpandedName::new("", "classid"),
            ExpandedName::new("", "codebase"),
            ExpandedName::new("", "data"),
            ExpandedName::new("", "usemap"),
            ExpandedName::new("", "fromaction"),
            ExpandedName::new("", "poster"),
            ExpandedName::new("", "archive"),
            ExpandedName::new(ns!(xlink), "href"),
        ])
    })
}

fn handle_style_sheet(css: &mut String) {
    let Ok(mut sheet) = StyleSheet::parse(
        css,
        ParserOptions {
            error_recovery: true,
            ..Default::default()
        },
    )
    .inspect_err(|e| {
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
    let Ok(mut style_attribute) = StyleAttribute::parse(
        css,
        ParserOptions {
            error_recovery: true,
            ..Default::default()
        },
    )
    .inspect_err(|e| {
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
        VisitTypes::URLS
            | VisitTypes::IMAGES
            | VisitTypes::FUNCTIONS
            | VisitTypes::VARIABLES
            | VisitTypes::PROPERTIES
            | VisitTypes::TOKENS
    }

    fn visit_url(&mut self, url: &mut Url<'i>) -> Result<(), Self::Error> {
        if !is_valid_url(&url.url) {
            url.url = String::new().into();
        }
        Ok(())
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
            self.visit_token_list(&mut function.arguments)?;
        }

        Ok(())
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
