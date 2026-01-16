//! Proton Mail Message Detector identifies previous messages in HTML content

#[cfg(test)]
#[path = "tests/message_detector.rs"]
mod tests;

use kuchikiki::{
    NodeData, NodeRef,
    iter::{NodeEdge, NodeIterator},
};

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

static BLOCKQUOTE_SELECTOR: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    BLOCKQUOTE_SELECTORS
        .map(|v| format!("{v}:not(:empty)"))
        .join(",")
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

fn strip_blockquote_inner(message: &NodeRef) -> Option<NodeRef> {
    let blockquote = message
        .inclusive_descendants()
        .select(&BLOCKQUOTE_SELECTOR)
        .inspect_err(|()| {
            tracing::warn!("Could not compile selector");
        })
        .ok()?
        // TODO: Whenever we update kuchikiki to a version that supports `:is` selector,
        // replace that ancestor traversal with `:not(:is(BLOCKQUOTE_SELECTOR) :is(BLOCKQUOTE_SELECTOR))
        .filter(move |quote| {
            // We want to filter out all quotes that are already inside of a quote.
            quote
                .as_node()
                .ancestors()
                // Select on an iterator acts like a filter. It does not traverse its descendants
                .select(&BLOCKQUOTE_SELECTOR)
                .ok()
                .and_then(|mut i| i.next())
                .is_none()
        })
        // We first focus on last blockquote, because the next step iterates on following nodes.
        // And only last blockquote can possibly have NO following nodes ;)
        .last()
        .and_then(move |last_blockquote| {
            if last_blockquote
                .as_node()
                .following_nodes()
                .any(|following_node| {
                    // If there is any text content - its not our blockquote
                    if !following_node.text_contents().trim().is_empty() {
                        return true;
                    }
                    // And if there is any image - then again its not our blockquote
                    // At this point we already replaced images with an anchor, but we want to keep them
                    following_node.select_first(".proton-image-anchor").is_ok()
                })
            {
                return None;
            }
            Some(last_blockquote)
        })
        .map(|n| n.as_node().to_owned());

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
