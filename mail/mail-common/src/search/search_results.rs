//! Search result processing for Foundation Search
//!
//! This module contains the logic to process raw search results from the
//! Foundation Search engine and convert them to a format suitable for the UI.
//!
//! The key responsibility is converting remote MessageIds (used by the search
//! engine) back to local MessageIds (used by the UI).

use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_search::{FoundEntry, MailSearchService};
use stash::orm::Model;
use stash::stash::Tether;

use crate::MailContextError;
use crate::datatypes::LocalMessageId;
use crate::models::Message;

/// A search result with highlighting metadata
#[derive(Clone, Debug)]
pub struct LocalSearchResult {
    /// The local message ID (converted from remote ID via database lookup)
    pub local_message_id: LocalMessageId,
    /// Relevance score from the search engine
    pub score: f64,
    /// Match positions for highlighting
    pub matches: Vec<SearchMatchPosition>,
}

/// The attribute where a search match was found
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMatchAttribute {
    /// Message body text
    Body,
    /// Message subject line
    Subject,
    /// Sender email address
    From,
    /// Primary recipients
    To,
    /// CC recipients
    Cc,
    /// BCC recipients
    Bcc,
}

impl SearchMatchAttribute {
    /// Parse an attribute string into the enum
    ///
    /// Returns `None` if the string doesn't match any known attribute.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "body" => Some(Self::Body),
            "subject" => Some(Self::Subject),
            "from" => Some(Self::From),
            "to" => Some(Self::To),
            "cc" => Some(Self::Cc),
            "bcc" => Some(Self::Bcc),
            _ => None,
        }
    }

    /// Convert the enum to its string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Subject => "subject",
            Self::From => "from",
            Self::To => "to",
            Self::Cc => "cc",
            Self::Bcc => "bcc",
        }
    }
}

impl std::fmt::Display for SearchMatchAttribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Position of a search match within document content
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchMatchPosition {
    /// The attribute where the match was found
    pub attribute: SearchMatchAttribute,
    /// Character position within the attribute value
    pub position: u64,
    /// Index for multi-value attributes
    pub value_index: u64,
}

/// Search the local index and convert results to local message IDs
///
/// This function:
/// 1. Calls the search service to get raw results with remote IDs
/// 2. Looks up each remote ID in the database to find the local ID
/// 3. Filters out results where the message no longer exists locally
/// 4. Returns results with local IDs suitable for the UI
pub async fn search_local_with_keywords(
    search_service: &MailSearchService,
    tether: &Tether,
    keywords: &str,
) -> Result<Vec<LocalSearchResult>, MailContextError> {
    // Get raw results from the search engine
    let found_entries = search_service
        .search_local_with_metadata(keywords)
        .await
        .map_err(|e| MailContextError::from(e.into_inner()))?;

    let mut results: Vec<LocalSearchResult> = Vec::new();

    for found in found_entries {
        // Process the raw FoundEntry
        if let Some(result) = process_found_entry(found, tether).await? {
            results.push(result);
        }
    }

    Ok(results)
}

/// Process a single FoundEntry and convert to LocalSearchResult
async fn process_found_entry(
    found: FoundEntry,
    tether: &Tether,
) -> Result<Option<LocalSearchResult>, MailContextError> {
    let identifier = found.identifier().to_string();
    let score = f64::from(found.score());

    // Extract all match occurrences with their positions
    let matches: Vec<SearchMatchPosition> = found
        .matches()
        .flat_map(|match_value| {
            match_value
                .occurrences()
                .into_iter()
                .filter_map(|occurrence| {
                    // Parse attribute string into enum, skip if unknown
                    SearchMatchAttribute::from_str(occurrence.attribute()).map(|attribute| {
                        SearchMatchPosition {
                            attribute,
                            position: occurrence.position().0 as u64,
                            value_index: occurrence.index().0 as u64,
                        }
                    })
                })
        })
        .collect();

    let remote_id_str = identifier.as_str();

    // Look up local ID from remote ID
    let remote_id = MessageId::from(remote_id_str.to_string());
    let Some(message) = Message::find_by_remote_id(remote_id, tether).await? else {
        // Message not found locally - skip this result
        return Ok(None);
    };

    Ok(Some(LocalSearchResult {
        local_message_id: message.id(),
        score,
        matches,
    }))
}
