//! Proton Mail Message Detector identifies previous messages in HTML content

#[cfg(test)]
mod test_messages;

use lazy_static::lazy_static;
use scraper::{ElementRef, Node, Selector};

const ORIGINAL_MESSAGE: &str = "------- Original Message -------";

const BLOCKQUOTE_SELECTORS: [&str; 22] = [
    ".protonmail_quote", // Proton Mail
    // Gmail creates both div.gmail_quote and blockquote.gmail_quote. The div
    // version marks text but does not cause indentation, but both should be
    // considered quoted text.
    ".gmail_quote",                                         // Gmail
    "div.gmail_extra",                                      // Gmail
    "div.yahoo_quoted",                                     // Yahoo Mail
    "blockquote.iosymail",                                  // Yahoo iOS Mail
    ".tutanota_quote",                                      // Tutanota Mail
    ".zmail_extra",                                         // Zoho
    ".skiff_quote",                                         // Skiff Mail
    "blockquote[data-skiff-mail]",                          // Skiff Mail
    r"#divRplyFwdMsg",                                      // Outlook Mail
    r#"div[id="mail-editor-reference-message-container"]"#, // Outlook
    r#"div[id="3D\"divRplyFwdMsg\""]"#,                     // Office365
    "hr[id=replySplit]",
    ".moz-cite-prefix",
    "div[id=isForwardContent]",
    "blockquote[id=isReplyContent]",
    "div[id=mailcontent]",
    "div[id=origbody]",
    "div[id=reply139content]",
    r"blockquote[id=oriMsgHtmlSeperator]",
    r#"blockquote[type="cite"]"#,
    r#"[name="quote"]"#, // gmx
];

const BLOCKQUOTE_TEXT_SELECTORS: [&str; 1] = [ORIGINAL_MESSAGE];

lazy_static! {
    static ref BLOCKQUOTE_SELECTOR: String = {
        BLOCKQUOTE_SELECTORS
            .map(|v| format!("{v}:not(:empty)"))
            .join(",")
    };
}

// When we try to determine what part of the body is the blockquote,
// We want to check that there is no text or no "important" element after the element we're testing
const ELEMENTS_AFTER_BLOCKQUOTES: [&str; 1] = [
    ".proton-image-anchor", // At this point we already replaced images with an anchor, but we want to keep them
];

//const searchForContent = (element: Element, text: string) => {
//    const xpathResult = element.ownerDocument?.evaluate(
//        `//*[text()='${text}']`,
//        element,
//        null,
//        XPathResult.ORDERED_NODE_ITERATOR_TYPE,
//        null
//    );
//    const result: Element[] = [];
//    let match = null;
//    // eslint-disable-next-line no-cond-assign
//    while ((match = xpathResult?.iterateNext())) {
//        result.push(match as Element);
//    }
//    return result;
//};

fn search_for_content<'a>(element: ElementRef<'a>, text: &'a str) -> Vec<ElementRef<'a>> {
    let mut result = Vec::new();
    for elem in element.traverse() {
        if let ego_tree::iter::Edge::Open(node) = elem {
            if let Node::Text(ref node_text) = node.value() {
                let node_text = &**node_text;
                if node_text == text {
                    if let Some(parent_node) = node.parent() {
                        if let Some(element_ref) = ElementRef::wrap(parent_node) {
                            result.push(element_ref);
                        }
                    }
                }
            }
        }
    }

    result
}

//
//     const parentHTML = tmpDocument.innerHTML || '';
//     let result: [string, string] | null = null;
//
//     const testBlockquote = (blockquote: Element) => {
//         const blockquoteHTML = blockquote.outerHTML || '';
//         const [beforeHTML = '', afterHTML = ''] = split(parentHTML, blockquoteHTML);
//
//         const after = parseStringToDOM(afterHTML);
//
//         // The "real" blockquote will be determined based on the fact:
//         // - That there is no text after the current blockquote element
//         // - That there is no "important" element after the current blockquote element
//         const hasImageAfter = after.body.querySelector(ELEMENTS_AFTER_BLOCKQUOTES.join(','));
//         const hasTextAfter = after.body?.textContent?.trim().length;
//
//         if (!hasImageAfter && !hasTextAfter) {
//             return [beforeHTML, blockquoteHTML] as [string, string];
//         }
//
//         return null;
//     };
//

// /**
//  * Returns content before and after match in the source
//  * Beware, String.prototype.split does almost the same but will not if there is several match
//  */
// export const split = (source: string, match: string): [string, string] => {
//     const index = source.indexOf(match);
//     if (index === -1) {
//         return [source, ''];
//     }
//     return [source.slice(0, index), source.slice(index + match.length)];
// };

fn test_block_quote(parent_html: &str, blockquote: ElementRef) -> Option<(String, String)> {
    let blockquote_html = blockquote.html();

    let (before_html, after_html) = parent_html.split_once(&blockquote_html).unwrap_or_default();
    let after = scraper::Html::parse_document(after_html);

    let elements_after_blockquotes_selector =
        Selector::parse(&ELEMENTS_AFTER_BLOCKQUOTES.join(",")).expect("Failed to build selector");

    let has_image_after = after
        .select(&elements_after_blockquotes_selector)
        .next()
        .is_some();

    let has_text_after = {
        let mut has_text = false;
        for text_element in after.root_element().text() {
            if !text_element.trim().is_empty() {
                has_text = true;
                break;
            }
        }

        has_text
    };

    if !has_image_after && !has_text_after {
        return Some((before_html.into(), blockquote_html));
    }

    None
}

///Try to locate the eventual blockquote present in the document no matter the expeditor of the mail
///
///Return the HTML content split at the blockquote start
///
/// # Panics
///
/// Will panic if it fails to parse the blockquote selector
#[must_use]
pub fn locate_blockquote(document: &str) -> (String, String) {
    // export const locateBlockquote = (inputDocument: Element | undefined): [content: string, blockquote: string] => {
    //     if (!inputDocument) {
    //         return ['', ''];
    //     }
    //

    let parsed_doc = scraper::Html::parse_document(document);

    //     const body = inputDocument.querySelector('body');
    //     const tmpDocument = body || inputDocument;

    let body_selector =
        scraper::Selector::parse("body").expect("failed to create selector for body");
    let root_element = if let Some(body) = parsed_doc.select(&body_selector).next() {
        body
    } else {
        parsed_doc.root_element()
    };

    //     // Standard search with a composed query selector
    //     const blockquotes = [...tmpDocument.querySelectorAll(BLOCKQUOTE_SELECTOR)];
    //     blockquotes.forEach((blockquote) => {
    //         if (result === null) {
    //             result = testBlockquote(blockquote);
    //         }
    //     });
    let blockquote_selector = scraper::Selector::parse(&BLOCKQUOTE_SELECTOR)
        .expect("failed to parse blockquote selector");

    let parent_html = root_element.inner_html();

    let mut result = None;

    for element in root_element.select(&blockquote_selector) {
        result = test_block_quote(&parent_html, element);
        if result.is_some() {
            break;
        }
    }

    //
    //     // Second search based on text content with xpath
    //     if (result === null) {
    //         BLOCKQUOTE_TEXT_SELECTORS.forEach((text) => {
    //             if (result === null) {
    //                 searchForContent(tmpDocument, text).forEach((blockquote) => {
    //                     if (result === null) {
    //                         result = testBlockquote(blockquote);
    //                     }
    //                 });
    //             }
    //         });
    //         // document.ownerDocument?.evaluate;
    //     }

    if result.is_none() {
        'outer: for text in BLOCKQUOTE_TEXT_SELECTORS {
            for element in search_for_content(root_element, text) {
                result = test_block_quote(&parent_html, element);
                if result.is_some() {
                    break 'outer;
                }
            }
        }
    }

    //     return result || [parentHTML, ''];
    result.unwrap_or_else(move || (parent_html, String::new()))
}

#[test]
fn detect_blockquote_or_signature() {
    let input = r#"<div style="font-family: verdana; font-size: 20px;">
    <div style="font-family: verdana; font-size: 20px;"><br></div>
    <div class="protonmail_signature_block protonmail_signature_block-empty" style="font-family: verdana; font-size: 20px;">
        <div class="protonmail_signature_block-user protonmail_signature_block-empty"></div>
        <div class="protonmail_signature_block-proton protonmail_signature_block-empty"></div>
    </div>
    <div style="font-family: verdana; font-size: 20px;"><br></div>
    <div class="protonmail_quote">
        On Tuesday, January 4th, 2022 at 17:13, Swiip - Test account &lt;swiip.test@protonmail.com&gt; wrote:<br>
        <blockquote class="protonmail_quote" type="cite">
            <div style="font-family: verdana; font-size: 20px;">
                <div style="font-family: verdana; font-size: 20px;">test</div>
                <div class="protonmail_signature_block protonmail_signature_block-empty" style="font-family: verdana; font-size: 20px;">
                    <div class="protonmail_signature_block-user protonmail_signature_block-empty"></div>
                    <div class="protonmail_signature_block-proton protonmail_signature_block-empty"></div>
                </div>
            </div>
        </blockquote><br>
    </div>
</div>"#;

    let (before, after) = locate_blockquote(input);

    assert!(!before.contains("On Tuesday"));
    assert!(after.contains("On Tuesday"));
}

#[test]
fn should_take_the_last_element_containing_text_in_case_of_sibling_blockquotes() {
    let input = r#"Email content
<div class="protonmail_quote">
    blockquote1
</div>
<div class="protonmail_quote">
    blockquote2
</div>"#;

    let (before, after) = locate_blockquote(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(!before.contains("blockquote2"));
    assert!(after.contains("blockquote2"));
    assert!(!after.contains("blockquote1"));
}

#[test]
fn should_take_the_last_element_containing_an_image_in_cas_of_sibling_blockquotes() {
    let input = r#"Email content
<div class="protonmail_quote">
    blockquote1
</div>
<div class="protonmail_quote">
    <span class="proton-image-anchor" />
</div>"#;

    let (before, after) = locate_blockquote(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(!before.contains("proton-image-anchor"));
    assert!(after.contains("proton-image-anchor"));
    assert!(!after.contains("blockquote1"));
}

#[test]
fn should_display_nothing_in_blockquote_when_there_is_text_after_blockquotes() {
    let input = r#"Email content
<div class="protonmail_quote">
    blockquote1
</div>
text after blockquote"#;

    let (before, after) = locate_blockquote(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(before.contains("text after blockquote"));
    assert!(after.is_empty());
}

#[test]
fn should_display_nothing_in_blockquote_when_there_is_an_image_after_blockquotes() {
    let input = r#"Email content
<div class="protonmail_quote">
    blockquote1
</div>
<span class="proton-image-anchor" />"#;

    let (before, after) = locate_blockquote(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(before.contains("proton-image-anchor"));
    assert!(after.is_empty());
}

#[test]
fn should_find_blockquote_in_mail() {
    for (idx, &mail) in test_messages::DEFAULT.iter().enumerate() {
        let (_, after) = locate_blockquote(mail);
        assert!(
            !after.is_empty(),
            "blockquote failed for message {idx}\n{mail}"
        );
    }
}
