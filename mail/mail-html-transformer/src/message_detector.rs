//! Proton Mail Message Detector identifies previous messages in HTML content

#[cfg(test)]
#[path = "tests/message_detector.rs"]
mod tests;

use kuchikiki::{NodeData, NodeRef, iter::NodeEdge};

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

/// This function returns Some if it finds a blockquote and strips it into a [`SplitDoc`]
fn find_split_doc(i: impl Iterator<Item = NodeRef>) -> Option<NodeRef> {
    i.filter(|blockquote| {
        // When we try to determine what part of the body is the blockquote,
        // At this point we already replaced images with an anchor, but we want to keep them
        if let Some(next) = blockquote
            .following_siblings() // The next sibling element
            .find(|x| matches!(x.data(), NodeData::Element(_)))
            && next
                .select(".proton-image-anchor")
                .unwrap()
                .next()
                .is_some()
        {
            // It has an image after
            return false;
        }
        // If it has text after
        if blockquote.following_siblings().any(|node_ref| {
            matches!(
                node_ref.data(),
                NodeData::Text(text) if !text.borrow().trim().is_empty()
            )
        }) {
            return false;
        }

        true
    })
    .last()
    .inspect(|blockquote| {
        blockquote.detach();
    })
}

/// Try to remove the blockquote and returns if it had it.
#[must_use]
pub fn strip_blockquote(message: NodeRef) -> bool {
    // First let's find an element with a well known selector
    // such as an element with class `protonmail_quote
    let i = message
        .select("body")
        .unwrap() // The selector is well formed
        .next() // Only one body ;)
        .unwrap() // Guaranteed to exist
        .as_node()
        .children() // We only care about the first-level children of the body
        .filter_map(|body_child| {
            body_child
                .select(&BLOCKQUOTE_SELECTOR)
                .unwrap()
                .next() // We want the top level element, not any children.
                .map(|x| x.as_node().to_owned())
        });

    if find_split_doc(i).is_some() {
        return true;
    }

    // We haven't found such a thing, let's see if we find the string
    // ------- Original Message ------- in a text node and get its parent.
    let i = message
        .traverse_inclusive()
        .filter_map(|node| match node {
            NodeEdge::Start(node_ref) => Some(node_ref),
            NodeEdge::End(_) => None,
        })
        .filter_map(|node_ref| match node_ref.data() {
            NodeData::Text(text) if *text.borrow() == "------- Original Message -------" => {
                node_ref
                    .parent() // Get tag
                    .and_then(|x| x.parent()) // Get partent
            }
            _ => None,
        });

    find_split_doc(i).is_some()
}

/// Try to locate the eventual blockquote present in the document no matter the expeditor of the mail
#[must_use]
pub fn locate_blockquote(message: NodeRef) -> SplitDoc {
    // First let's find an element with a well known selector
    // such as an element with class `protonmail_quote
    let i = message
        .select("body")
        .unwrap() // The selector is well formed
        .next() // Only one body ;)
        .unwrap() // Guaranteed to exist
        .as_node()
        .children() // We only care about the first-level children of the body
        .filter_map(|body_child| {
            body_child
                .select(&BLOCKQUOTE_SELECTOR)
                .unwrap()
                .next() // We want the top level element, not any children.
                .map(|x| x.as_node().to_owned())
        });
    if let Some(blockquote) = find_split_doc(i) {
        return SplitDoc {
            message,
            blockquote: Some(blockquote),
        };
    }

    // We haven't found such a thing, let's see if we find the string
    // ------- Original Message ------- in a text node and get its parent.
    let i = message
        .traverse_inclusive()
        .filter_map(|node| match node {
            NodeEdge::Start(node_ref) => Some(node_ref),
            NodeEdge::End(_) => None,
        })
        .filter_map(|node_ref| match node_ref.data() {
            NodeData::Text(text) if *text.borrow() == "------- Original Message -------" => {
                node_ref
                    .parent() // Get tag
                    .and_then(|x| x.parent()) // Get partent
            }
            _ => None,
        });

    if let Some(blockquote) = find_split_doc(i) {
        return SplitDoc {
            message,
            blockquote: Some(blockquote),
        };
    }

    // No luck
    SplitDoc {
        message,
        blockquote: None,
    }
}
