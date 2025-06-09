#[cfg(test)]
#[path = "tests/sanitizer.rs"]
mod tests;

use kuchikiki::{NodeData, NodeRef, iter::NodeEdge};
use std::{collections::HashSet, sync::LazyLock};
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

static ATTR_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    hash_set! {
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
        "style",
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
    }
});

/// Tags that should be removed with their inner HTML.
static TAGS_TO_REMOVE_WITH_INNER_HTML: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    hash_set! {
        "script"
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

                let mut attrs = e.attributes.borrow_mut();
                attrs.map.retain(|name, _| ATTR_SET.contains(&&*name.local));
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
