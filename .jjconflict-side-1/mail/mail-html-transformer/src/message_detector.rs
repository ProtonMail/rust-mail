//! Proton Mail Message Detector identifies previous messages in HTML content

#[cfg(test)]
#[path = "tests/message_detector.rs"]
mod tests;

use kuchikiki::{NodeData, NodeRef, Selectors, iter::NodeEdge};

use crate::utils::NodeRefExt;

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

static BLOCKQUOTE_SELECTOR: std::sync::LazyLock<Option<Selectors>> =
    std::sync::LazyLock::new(|| {
        let selectors_string = BLOCKQUOTE_SELECTORS
            .map(|v| format!("{v}:not(:empty)"))
            .join(",");

        Selectors::compile(&selectors_string)
            .inspect_err(|()| {
                tracing::warn!("Could not compile selector");
            })
            .ok()
    });

/// This is the result of calling [`locate_blockquote`].
pub struct SplitDoc {
    /// The HTML after the blockquote has been stripped
    pub message: NodeRef,
    /// The HTML of the blockquote if it has found it.
    pub blockquote: Option<NodeRef>,
}

/// Try to remove the blockquote and returns if it had it.
#[must_use]
pub fn strip_blockquote(message: NodeRef) -> SplitDoc {
    let blockquote = strip_blockquote_inner(&message);
    SplitDoc {
        message,
        blockquote,
    }
}

fn traverse_blockquotes(message: NodeRef, selectors: &Selectors) -> impl Iterator<Item = NodeRef> {
    let mut node = Some(message);

    std::iter::from_fn(move || {
        loop {
            let current = node.take()?;
            if matches_node_ref(current.clone(), selectors) {
                // Its a match, we dont want to go deeper, so we take next node instead
                node = current.following_nodes().next();
                return Some(current);
            } else if let Some(child) = current.first_child() {
                node = Some(child);
            } else {
                // Node was childless, we need to process next nodes
                node = current.following_nodes().next();
            }
        }
    })
}

fn matches_node_ref(node_ref: NodeRef, selectors: &Selectors) -> bool {
    let Some(element_ref) = node_ref.into_element_ref() else {
        return false;
    };
    selectors.matches(&element_ref)
}

fn strip_blockquote_inner(message: &NodeRef) -> Option<NodeRef> {
    let selectors = BLOCKQUOTE_SELECTOR.as_ref()?;

    let blockquote = traverse_blockquotes(message.clone(), selectors)
        // We first focus on last blockquote, because the next step iterates on following nodes.
        // And only last blockquote can possibly have NO following nodes ;)
        .last()
        .and_then(move |last_blockquote| {
            if last_blockquote.following_nodes().any(|following_node| {
                // If there is any text content - its not our blockquote
                if !following_node.text_contents().trim().is_empty() {
                    return true;
                }
                // And if there is any image - then again its not our blockquote
                // At this point we already replaced images with an anchor, but we want to keep them
                following_node.select_first(".proton-image-anchor").is_ok()
            }) {
                return None;
            }
            Some(last_blockquote)
        });

    // First let's find an element with a well known selector
    // such as an element with class `protonmail_quote
    if let Some(blockquote) = blockquote {
        blockquote.detach();
        return Some(blockquote);
    }

    // We haven't found such a thing, let's see if we find the string
    // ------- Original Message ------- in a text node and get its parent.
    let text_quote = message
        .traverse_inclusive()
        .filter_map(|node| match node {
            NodeEdge::Start(node_ref) => Some(node_ref),
            NodeEdge::End(_) => None,
        })
        .filter_map(|node_ref| match node_ref.data() {
            NodeData::Text(text) if text.borrow().trim() == ORIGINAL_MESSAGE => {
                node_ref
                    .parent() // Get tag
                    .and_then(|x| x.parent()) // Get partent
            }
            _ => None,
        })
        .last();

    if let Some(text_quote) = text_quote {
        text_quote.detach();
        return Some(text_quote);
    }
    None
}
