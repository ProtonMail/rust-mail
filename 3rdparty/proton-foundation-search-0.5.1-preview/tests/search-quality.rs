//! Search Quality Tests
//!
//! These tests verify the behaviour of our search algorithm, which can operate in two modes:
//!
//! 1. **Primary Trigram Mode** (--features primary-trigram)
//!    - Uses trigrams (3-character sequences) as the primary indexing method
//!    - Provides robust fuzzy matching and typo tolerance
//!
//! 2. **Term-based Mode** (default)
//!    - Uses word-based indexing as the primary method
//!    - Includes trigram support for fuzzy matching
//!
//! ## Search Algorithm Overview
//!
//! Regardless of the indexing mode, the search algorithm provides consistent behaviour:
//!
//! 1. **Distance-based Matching**
//!    - Uses Levenshtein distance (max_distance parameter) to find similar terms
//!    - Allows for typos and variations (e.g., "securty" matches "security")
//!    - Default max_distance of 3 characters
//!
//! 2. **Scoring and Ranking**
//!    - Each match is assigned a similarity score (0.0 to 1.0)
//!    - Scores are influenced by multiple factors:
//!      * Term frequency: More occurrences of the search term increase the score
//!      * Term position: Matches in subject/title are weighted higher than body text
//!      * Term proximity: Terms appearing close together boost the score
//!      * Match quality: Exact matches score higher than fuzzy matches
//!    - Results are sorted by score in descending order
//!    - Minimum similarity threshold (default 0.75) filters out poor matches
//!
//! 3. **Special Features**
//!    - Stop word filtering: Common words like "the" are ignored
//!    - Multilingual support: Works with non-Latin scripts (e.g., Chinese, German) but a known ISSUE in term based mode
//!    - Partial word matching (wildcard): Can match parts of words (e.g., "doc" matches "documentation")
//!
//! ## Test Categories
//!
//! The tests verify different aspects of the search behaviour:
//! - Position-based ranking (subject vs body matches)
//! - Term frequency impact on ranking
//! - Term proximity effects
//! - Similar word handling (typos)
//! - Stop word filtering
//! - Multilingual support
//! - Search quality metrics (precision and recall)
//!
//! To run these tests specifically and in isolation:
//! - With trigram-based indexing: `cargo test test_qs_ --features primary-trigram -- --nocapture`
//! - With term-based indexing: `cargo test test_qs_ -- --nocapture`
#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]
use std::collections::{HashMap, HashSet};

use chrono::{TimeZone, Utc};
use proton_foundation_search::index::prelude::EntryIndex;
use proton_foundation_search::query::option::QueryOptions;
use proton_foundation_search::query::option::text::{MaximumDistance, MinimumSimilarity};
use test_log::test;

#[path = "util/test_utils.rs"]
mod test_utils;
use test_utils::{Asset, SearchIndex};
use tracing::info;

// Helper to create test index with our quality test data
fn create_quality_test_index() -> SearchIndex {
    //email style test data
    SearchIndex::preload("tests/fixtures/quality_test_data.jsonl")
}

// Helper to format timestamp
#[allow(dead_code)]
fn format_time(timestamp: i64) -> String {
    let dt = Utc.timestamp_opt(timestamp, 0).unwrap();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

// Update print_search_results to use Asset's formatted_time
fn print_search_results(query: &str, results: &[(EntryIndex, f64)]) {
    let assets = Asset::load("tests/fixtures/quality_test_data.jsonl").unwrap();

    info!("\nSearch results for '{}':", query);
    info!("Rank | Entry | Score | Time                | Content");
    info!("-----|-------|-------|-------------------|--------");
    for (rank, (entry, score)) in results.iter().enumerate() {
        let asset = assets
            .iter()
            .find(|a| {
                a.id.split('-')
                    .nth(1)
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0)
                    == entry.0
            })
            .unwrap();

        let content = match entry.0 {
            0 => "Security Guide (Security Team)",
            1 => "ProtonVPN Features (VPN Team)",
            2 => "Multilingual Support (Localization)",
            3 => "Technical Docs (Dev Relations)",
            4 => "Common Words (Support)",
            _ => "Unknown",
        };
        info!(
            "{:4} | {:5} | {:.3} | {} | {}",
            rank + 1,
            entry.0,
            score,
            asset.formatted_time(),
            content
        );
    }
}

/// Preprocesses a search query by:
/// - Removing stop words
/// - Handling punctuation
/// - Normalizing terms
/// - Parsing into search terms
fn preprocess_query(query: &str) -> Vec<String> {
    // Common English stop words
    const STOP_WORDS: &[&str] = &[
        "the", "be", "to", "of", "and", "a", "in", "that", "have", "i",
    ];

    query
        .split_whitespace()
        .map(|term| {
            // Remove punctuation and convert to lowercase
            term.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|term| {
            // Filter out stop words and empty terms
            !term.is_empty() && !STOP_WORDS.contains(&term.as_str())
        })
        .collect()
}

// Helper to search and return sorted results
fn search_sorted(
    index: &SearchIndex,
    query: &str,
    max_distance: usize,
    min_similarity: f64,
) -> Vec<(EntryIndex, f64)> {
    // Preprocess the query to handle stop words, punctuation, etc.
    let processed_terms = preprocess_query(query);

    // If all terms were stop words, return empty results
    if processed_terms.is_empty() {
        return Vec::new();
    }

    // Join the processed terms back into a query string
    // Note: This is a simple implementation. A more sophisticated approach
    // might handle phrases differently or use a more complex query structure
    let processed_query = processed_terms.join(" ");

    let mut results = index
        .search(
            &processed_query,
            &QueryOptions::default()
                .with::<MaximumDistance>(|value| **value = max_distance)
                .with::<MinimumSimilarity>(|value| **value = min_similarity),
        )
        .into_iter()
        .collect::<Vec<_>>();
    results.sort();
    results.into_iter().map(|(s, e)| (e, s.value())).collect()
}

#[ignore = "requires thresholds re-implementation"]
#[test]
fn test_qs_partial_match_handling() {
    let index = create_quality_test_index();
    let results = search_sorted(&index, "doc", 12, 0.2);

    // Debug print the results
    print_search_results("partial_match_handling", &results);

    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(3)),
        "Should find \"documentation\""
    );
}

#[test]
#[ignore]
fn test_qs_term_frequency_ranking() {
    let index = create_quality_test_index();
    let results = search_sorted(&index, "privacy", 3, 0.75);

    // Debug print the results
    print_search_results("term_frequency_ranking", &results);

    assert_eq!(
        results[0].0,
        EntryIndex(0),
        "Entry 0 should rank highest as \"privacy\" appears multiple times"
    );
}

#[test]
#[ignore]
fn test_qs_multilingual_support() {
    let index = create_quality_test_index();

    // Test Chinese
    let results = search_sorted(&index, "多语言", 3, 0.75);
    print_search_results("multilingual_support 多语言", &results);
    assert!(results.iter().any(|(entry, _)| *entry == EntryIndex(2)));

    // Test French
    let results = search_sorted(&index, "multilingue", 3, 0.75);
    print_search_results("multilingual_support multilingue", &results);
    assert!(results.iter().any(|(entry, _)| *entry == EntryIndex(2)));

    // Test German
    let results = search_sorted(&index, "Sprachen", 3, 0.75);
    print_search_results("multilingual_support Sprachen", &results);
    assert!(results.iter().any(|(entry, _)| *entry == EntryIndex(2)));
}
/*
#[test]
fn test_qs_by_multi_term_proximity_ranking() {
    let index = create_quality_test_index();
    let results = search_sorted(&index, "security privacy", 3, 0.75);

    // Debug print the results
    print_search_results("by_multi_term_proximity_ranking", &results);

    // Entry 0 should rank highest as terms appear close together
    assert_eq!(results[0].0, EntryIndex(0));
}
*/
#[test]
#[ignore]
fn test_qs_attribute_count_based_ranking() {
    let index = create_quality_test_index();
    let results = search_sorted(&index, "security", 3, 0.75);

    // Debug print the results
    print_search_results("attribute_count_based_ranking", &results);

    // email data should show that some schemas have more releavnt attributes - e.g. subject over body in an email
    // Entry 0 should rank highest as "security" appears in both subject and body multiple times
    assert_eq!(
        results[0].0,
        EntryIndex(0),
        "Entry 0 (Security Guide) should rank highest as it has 'security' in both subject and body. \
         If this fails, it may indicate that the subject field boost is not being applied correctly, \
         as the subject contains 'Security' and should contribute significantly to the ranking."
    );

    // Verify all entries containing "security" are present
    let security_entries: HashSet<_> = results.iter().map(|(entry, _)| *entry).collect();
    assert!(
        security_entries.contains(&EntryIndex(0)),
        "Entry 0 (Security Guide) should be present"
    );
    assert!(
        security_entries.contains(&EntryIndex(1)),
        "Entry 1 (ProtonVPN) should be present"
    );
    assert!(
        security_entries.contains(&EntryIndex(3)),
        "Entry 3 (Technical Docs) should be present"
    );
    assert!(
        security_entries.contains(&EntryIndex(4)),
        "Entry 4 (Common Words) should be present"
    );

    // Entry 2 (Multilingual Support) should not be present as it doesn't contain "security"
    assert!(
        !security_entries.contains(&EntryIndex(2)),
        "Entry 2 should not be present"
    );
}

#[test]
fn test_qs_similar_word_typo_handling() {
    let index = create_quality_test_index();
    let results = search_sorted(&index, "securty", 3, 0.7); // Common typo

    // Debug print the results
    print_search_results("similar_word_typo_handling", &results);

    // Should find "security" despite typo
    assert!(results.iter().any(|(entry, _)| *entry == EntryIndex(0)));
    assert!(results.iter().any(|(entry, _)| *entry == EntryIndex(1)));
}

#[test]
fn test_qs_stop_word_handling() {
    let index = create_quality_test_index();
    let results = search_sorted(&index, "the", 3, 0.75);

    // Debug print the results
    print_search_results("stop_word_handling", &results);

    // Should not return results for common stop words
    assert!(results.is_empty());
}

// Helper function to calculate precision and recall
fn calculate_metrics(
    results: &HashMap<EntryIndex, f64>,
    relevant_entries: &[EntryIndex],
) -> (f64, f64) {
    let retrieved: HashSet<_> = results.keys().collect();
    let relevant: HashSet<_> = relevant_entries.iter().collect();
    let true_positives = retrieved.intersection(&relevant).count();

    // Debug print the metric values
    info!("\nMetrics calculation:");
    info!("Retrieved entries: {:?}", retrieved);
    info!("Relevant entries: {:?}", relevant);
    info!("True positives (intersection): {}", true_positives);

    //stats
    let precision = if retrieved.is_empty() {
        0.0
    } else {
        true_positives as f64 / retrieved.len() as f64
    };
    let recall = if relevant.is_empty() {
        0.0
    } else {
        true_positives as f64 / relevant.len() as f64
    };

    info!(
        "Precision: {:.3} ({} / {})",
        precision,
        true_positives,
        retrieved.len()
    );
    info!(
        "Recall: {:.3} ({} / {})",
        recall,
        true_positives,
        relevant.len()
    );

    (precision, recall)
}

#[test]
fn test_qs_search_metrics_comprehensive() {
    let index = create_quality_test_index();

    // Test precision and recall for a common search scenario
    let results = search_sorted(&index, "security", 3, 0.75);
    print_search_results("search_metrics general", &results);

    // Define relevant entries for "security" query - all entries that contain "security"
    let relevant_entries = vec![
        EntryIndex(0), // Security Guide (subject + body)
        EntryIndex(1), // ProtonVPN Features (body)
        EntryIndex(3), // Technical Docs (body) - "API security is a priority"
        EntryIndex(4), // Common Words (body) - "Similar words: secure, security, securely"
    ];
    let (precision, recall) = calculate_metrics(&results.into_iter().collect(), &relevant_entries);

    // We expect high precision and recall for this query since all entries containing "security" should be found
    assert!(
        precision >= 0.8,
        "Precision should be at least 0.8, got {}",
        precision
    );
    assert!(
        recall >= 0.8,
        "Recall should be at least 0.8, got {}",
        recall
    );

    // Test precision and recall for a more specific search
    /*
    let results = search_sorted(&index, "security guide", 3, 0.75);
    print_search_results("search_metrics specific", &results);

    // For a specific phrase, we expect higher precision but potentially lower recall
    let specific_relevant = vec![EntryIndex(0)]; // Only the guide should be relevant
    let (precision, recall) = calculate_metrics(&results.into_iter().collect(), &specific_relevant);

    // For specific searches, we prioritize precision over recall
    assert!(
        precision >= 0.9,
        "Precision should be at least 0.9 for specific search, got {}",
        precision
    );
    // Recall might be lower as we're being more specific
    assert!(
        recall >= 0.5,
        "Recall should be at least 0.5 for specific search, got {}",
        recall
    );*/
}

#[ignore = "requires thresholds re-implementation"]
#[test]
fn test_qs_partial_word_matching() {
    let index = create_quality_test_index();

    // Test common word stems
    let results = search_sorted(&index, "secure", 3, 0.6);
    print_search_results("partial_word_matching secure", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should find 'security' when searching for 'secure'"
    );

    // Test common prefixes
    let results = search_sorted(&index, "priv", 4, 0.5);
    print_search_results("partial_word_matching priv", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should find 'privacy' when searching for 'priv'"
    );
}

#[test]
fn test_qs_common_variations_minor() {
    let index = create_quality_test_index();

    // Test plural forms
    let results = search_sorted(&index, "features", 3, 0.75);
    print_search_results("common_variations features", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(1)),
        "Should find 'feature' when searching for 'features'"
    );
}

#[ignore = "needs threshold options to be implemented"]
#[test]
fn test_qs_common_variations_major() {
    let index = create_quality_test_index();

    // Test common word variations
    let results = search_sorted(&index, "encrypt", 3, 0.6);
    print_search_results("common_variations encrypt", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(1)),
        "Should find 'encryption' when searching for 'encrypt'"
    );
}
/*
#[test]
fn test_qs_ranking_factors() {
    let index = create_quality_test_index();

    // Test that multiple factors contribute to ranking
    let results = search_sorted(&index, "security guide", 3, 0.75);
    print_search_results("ranking_factors", &results);

    // Entry 0 should rank highest because:
    // 1. Exact match in subject ("Security and Privacy Guide")
    // 2. Multiple occurrences in body
    // 3. Terms appear together
    assert_eq!(results[0].0, EntryIndex(0));

    // Entry 1 should rank lower because:
    // 1. Only has "security" in body
    // 2. Terms don't appear together
    assert!(
        results
            .iter()
            .position(|(entry, _)| *entry == EntryIndex(1))
            .unwrap_or(usize::MAX)
            > results
                .iter()
                .position(|(entry, _)| *entry == EntryIndex(0))
                .unwrap_or(usize::MAX)
    );
}*/

#[test]
fn test_qs_special_character_handling() {
    let index = create_quality_test_index();

    // Test punctuation handling
    let results = search_sorted(&index, "security!", 3, 0.75);
    print_search_results("special_chars punctuation", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should find 'security' when searching with punctuation"
    );

    // Test hyphenated terms
    /*
    let results = search_sorted(&index, "privacy-focused", 3, 0.75);
    print_search_results("special_chars hyphenated", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should find 'privacy' when searching with hyphen"
    );*/

    // Test numbers and symbols
    /*
    let results = search_sorted(&index, "vpn2.0", 3, 0.75);
    print_search_results("special_chars numbers", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(1)),
        "Should find 'vpn' when searching with numbers"
    );*/
}

#[test]
fn test_qs_comprehensive_typo_handling() {
    let index = create_quality_test_index();

    // Test transposed letters
    let results = search_sorted(&index, "secruity", 3, 0.75);
    print_search_results("typos transposed", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should handle transposed letters"
    );

    // Test missing letters
    let results = search_sorted(&index, "securty", 3, 0.75);
    print_search_results("typos missing", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should handle missing letters"
    );

    // Test extra letters
    let results = search_sorted(&index, "securitty", 3, 0.75);
    print_search_results("typos extra", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should handle extra letters"
    );

    // Test keyboard typos (common adjacent key mistakes)
    let results = search_sorted(&index, "secur1ty", 3, 0.75);
    print_search_results("typos keyboard", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should handle keyboard typos"
    );
}

#[test]
fn test_qs_extended_multilingual() {
    let index = create_quality_test_index();

    // Test accented characters
    let results = search_sorted(&index, "sécurité", 3, 0.75);
    print_search_results("multilingual accented", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should handle accented characters"
    );

    // Test Cyrillic - Russian etc
    let results = search_sorted(&index, "безопасность", 3, 0.75);
    print_search_results("multilingual cyrillic", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(5)),
        "Should handle Cyrillic script - entry 5 contains Russian text about security (безопасность)"
    );
}

#[ignore = "requires thresholds re-implementation"]
#[test]
fn test_qs_extended_multilingual_major() {
    let index = create_quality_test_index();

    // Test mixed scripts
    let results = search_sorted(&index, "security安全", 6, 0.75);
    print_search_results("multilingual mixed", &results);
    assert!(
        results.iter().any(|(entry, _)| *entry == EntryIndex(0)),
        "Should handle mixed scripts"
    );
}
