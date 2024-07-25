//https://github.com/rust-lang/rust-clippy/issues/13155
#![allow(clippy::needless_pass_by_value)]
use std::collections::HashSet;

use kuchikiki::{iter::NodeEdge, Node, NodeData, NodeRef};

lazy_static::lazy_static! {
    static ref TAG_SET: HashSet<&'static str> = TAGS.into();
    static ref ATTR_SET: HashSet<&'static str> = ATTRS.into();
}

pub fn strip_whitelist(doc: NodeRef) {
    doc.traverse_inclusive()
        .filter_map(|node| match node {
            NodeEdge::Start(node_ref) => Some(node_ref),
            NodeEdge::End(_) => None,
        })
        .filter_map(|node_ref| match node_ref.data() {
            NodeData::Element(e) => {
                let tag_name: &str = &e.name.local;

                if !TAG_SET.contains(tag_name) {
                    return Some(node_ref);
                }

                let mut attrs = e.attributes.borrow_mut();
                attrs.map.retain(|name, _| ATTR_SET.contains(&&*name.local));
                None
            }
            _ => None,
        })
        .for_each(|node| node.detach());
}

pub const ATTRS: [&str; 112] = [
    "proton-src",
    "target",
    "accept",
    "action",
    "align",
    "alt",
    "autocapitalize",
    "autocomplete",
    "autopictureinpicture",
    "autoplay",
    "background",
    "bgcolor",
    "border",
    "capture",
    "cellpadding",
    "cellspacing",
    "checked",
    "cite",
    "class",
    "clear",
    "color",
    "cols",
    "colspan",
    "controls",
    "controlslist",
    "coords",
    "crossorigin",
    "datetime",
    "decoding",
    "default",
    "dir",
    "disabled",
    "disablepictureinpicture",
    "disableremoteplayback",
    "download",
    "draggable",
    "enctype",
    "enterkeyhint",
    "face",
    "headers",
    "height",
    "hidden",
    "high",
    "href",
    "hreflang",
    "id",
    "inputmode",
    "integrity",
    "ismap",
    "kind",
    "label",
    "lang",
    "list",
    "loading",
    "loop",
    "low",
    "max",
    "maxlength",
    "media",
    "method",
    "min",
    "minlength",
    "multiple",
    "muted",
    "name",
    "nonce",
    "noshade",
    "novalidate",
    "nowrap",
    "open",
    "optimum",
    "pattern",
    "placeholder",
    "playsinline",
    "popover",
    "popovertarget",
    "popovertargetaction",
    "poster",
    "preload",
    "pubdate",
    "radiogroup",
    "readonly",
    "rel",
    "required",
    "rev",
    "reversed",
    "role",
    "rows",
    "rowspan",
    "spellcheck",
    "scope",
    "selected",
    "shape",
    "size",
    "sizes",
    "span",
    "srclang",
    "start",
    "src",
    "step",
    "summary",
    "tabindex",
    "title",
    "translate",
    "type",
    "usemap",
    "valign",
    "value",
    "width",
    "wrap",
    "xmlns",
    "slot",
];

pub const TAGS: [&str; 116] = [
    "proton-src",
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
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
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
];

#[cfg(test)]
mod test {
    use crate::Transformer;

    #[test]
    fn acceptable_html() {
        let html = include_str!("../tests/htmls/acceptable.html");

        let unsanitized_html = Transformer::new(html).strip_whitelist().to_string();
        let html = Transformer::new(html).strip_whitelist().to_string();
        assert_eq!(unsanitized_html, html);
    }

    #[test]
    fn strip_bad_html() {
        let html = include_str!("../tests/htmls/strip_bad.html");

        let html = Transformer::new(html).strip_whitelist().to_string();
        insta::assert_snapshot!(html);
    }

    #[test]
    fn email_privacy_tester() {
        let html = include_str!("../tests/htmls/email_privacy_tester.html");

        let html = Transformer::new(html).strip_whitelist().to_string();
        insta::assert_snapshot!(html);
    }
}
