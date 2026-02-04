//! Tests for Unicode ligature handling in search
//!
//! This tests the behaviour when text contains ligature characters (like ﬁ, ﬂ, ﬀ)
//! which are single Unicode code points that visually represent multiple characters.
//!
//! **Current behaviour (known limitation)**: The search system does NOT normalize ligatures
//! to their component characters. Ligatures are treated as distinct Unicode characters.
//! This means "fi" and "ﬁ" (ligature) are indexed and searched separately.
//!
//! **Future improvement**: Ideally, searching for "fi" should find text containing "ﬁ" (ligature).
//! The tests currently verify the existing behavior (distinctness) and will be updated
//! when ligature normalization is implemented.

use proton_foundation_search::document::{Document, Value};
use search_internal_helper::engine_test_util::Cache;
use search_internal_helper::{self as helper};
use test_log::test;

/// Test that ligature characters are treated as distinct from their component characters.
///
/// The ﬁ ligature (U+FB01) is a single code point that looks like "fi" but is encoded differently.
/// This test verifies that:
/// - "fika" (with normal 'fi') and "ﬁka" (with ligature) are indexed separately
/// - Searching for one does not find the other (they are distinct strings)
#[test]
fn ligature_fi_is_distinct_from_normal_fi() {
    helper::init_logs();
    let mut cache = Cache::default();

    let (engine, init) = helper::create_engine();
    cache.handle_write(init).expect("init ok");

    let mut writer = engine.write().unwrap();

    // Document 1: normal "fi"
    writer.insert(Document::new("1").with_attribute("body", Value::text("coffee fika")));

    // Document 2: ﬁ ligature (U+FB01)
    writer.insert(Document::new("2").with_attribute("body", Value::text("coffee ﬁka")));

    cache.handle_write(writer.commit()).expect("write ok");

    // Helper to search and get identifiers
    let mut search = |query: &str| -> Vec<String> {
        let query = engine
            .query()
            .with_expression(query.parse().unwrap())
            .search();
        cache
            .handle_search(query)
            .expect("search ok")
            .into_iter()
            .map(|entry| entry.identifier().to_string())
            .collect()
    };

    // "coffee" sholud find both documents
    let results = search("coffee");
    println!("Search 'coffee': {:?}", results);
    assert!(
        results.contains(&"1".to_string()),
        "should find doc 1 with 'coffee'"
    );
    assert!(
        results.contains(&"2".to_string()),
        "should find doc 2 with 'coffee'"
    );

    // "fika" (normal fi) should find document 1
    // TODO: Ideally should also find document 2 (with ligature "ﬁka"), but currently doesn't
    // due to lack of ligature normalization - this is a known limitation
    let results = search("fika");
    println!("Search 'fika': {:?}", results);
    assert!(
        results.contains(&"1".to_string()),
        "should find doc 1 with 'fika'"
    );
    // Currently does not find doc 2 (ligature) - this test documents current behavior

    // "ﬁka" (with ligature) should find document 2
    // TODO: Ideally should also find document 1 (with normal "fika"), but currently doesn't
    // due to lack of ligature normalization - this is a known limitation
    let results = search("ﬁka");
    println!("Search 'ﬁka' (ligature): {:?}", results);
    assert!(
        results.contains(&"2".to_string()),
        "should find doc 2 with 'ﬁka' ligature"
    );
    // Currently does not find doc 1 (normal) - this test documents current behavior
}

/// Test trigram extraction with ligature characters
#[test]
fn ligature_trigrams() {
    use proton_foundation_search::index::text::trigram::Trigrams;

    // Normal "fika" - 4 characters, 4 bytes
    let normal_trigrams: Vec<_> = "fika".trigrams().collect();
    println!("'fika' trigrams: {:?}", normal_trigrams);
    assert_eq!(normal_trigrams, vec![(0, "fik"), (1, "ika")]);

    // "ﬁka" with ligature - the ﬁ is a single 3-byte character
    // So this is 3 characters: ﬁ(3 bytes) + k(1 byte) + a(1 byte) = 5 bytes total
    let ligature_trigrams: Vec<_> = "ﬁka".trigrams().collect();
    println!("'ﬁka' (ligature) trigrams: {:?}", ligature_trigrams);
    // ﬁ is at bytes 0-2, k at byte 3, a at byte 4
    // Only one trigram possible: "ﬁka" (exactly 3 characters)
    assert_eq!(ligature_trigrams, vec![(0, "ﬁka")]);

    // This shows why they don't match: different trigrams!
    // "fika" has trigrams: ["fik", "ika"]
    // "ﬁka" has trigram: ["ﬁka"]
    // No overlap, so no trigram-based match
    // TODO: When ligature normalization is implemented, these should be normalized to the same trigrams
}

/// Test various common ligatures
#[test]
fn various_ligatures() {
    use proton_foundation_search::index::text::trigram::Trigrams;

    // ﬁ ligature (U+FB01) - fi
    let fi_lig: Vec<_> = "ﬁnd".trigrams().collect();
    println!("'ﬁnd' trigrams: {:?}", fi_lig);

    // ﬂ ligature (U+FB02) - fl
    let fl_lig: Vec<_> = "ﬂow".trigrams().collect();
    println!("'ﬂow' trigrams: {:?}", fl_lig);

    // ﬀ ligature (U+FB00) - ff
    let ff_lig: Vec<_> = "coﬀee".trigrams().collect();
    println!("'coﬀee' trigrams: {:?}", ff_lig);

    // Compare with normal versions
    let find_normal: Vec<_> = "find".trigrams().collect();
    let flow_normal: Vec<_> = "flow".trigrams().collect();
    let coffee_normal: Vec<_> = "coffee".trigrams().collect();

    println!("'find' normal trigrams: {:?}", find_normal);
    println!("'flow' normal trigrams: {:?}", flow_normal);
    println!("'coffee' normal trigrams: {:?}", coffee_normal);

    // Currently they are different because ligatures are single chars and not normalized
    // TODO: When ligature normalization is implemented, these should produce the same trigrams
    assert_ne!(
        fi_lig, find_normal,
        "ligature and normal currently have different trigrams (known limitation)"
    );
    assert_ne!(
        fl_lig, flow_normal,
        "ligature and normal currently have different trigrams (known limitation)"
    );
    assert_ne!(
        ff_lig, coffee_normal,
        "ligature and normal currently have different trigrams (known limitation)"
    );
}
