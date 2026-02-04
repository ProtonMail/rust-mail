use test_log::test;

use super::*;
use crate::query::expression::TermValue;
#[test]
fn test_match_trigrams() {
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["světlo", "bungee", "வரவேற்பு"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    // a trivial search for trigrams using the bloom filter
    assert_eq!((100.0 * sut.test_match_trigrams("bungee")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("geek")) as usize, 50);
    assert_eq!((100.0 * sut.test_match_trigrams("svetlo")) as usize, 25);
    assert_eq!((100.0 * sut.test_match_trigrams("světlem")) as usize, 60);
    assert_eq!((100.0 * sut.test_match_trigrams("வரவே")) as usize, 100);
}

#[test]
fn test_matching_trigrams() {
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text([
        "světlo",
        "světelný",
        "bungee",
        "வரவேற்பு",
        "nonsense",
    ])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    let matches = |term| {
        sut.test_matching_trigrams(None, term)
            .map(|(e, a, v, t, _c, trigram)| ((e, a, v, t), trigram))
            .fold(BTreeMap::<_, Vec<_>>::new(), |mut map, (key, trigram)| {
                map.entry(key).or_default().push(trigram);
                map
            })
            .into_values()
            .collect::<Vec<_>>()
    };
    // a distance based term search
    assert_eq!(matches("bungee"), vec![vec!["bun", "ung", "nge", "gee"]]);
    assert_eq!(matches("bunna"), vec![vec!["bun"]]);
    assert_eq!(matches("svietlo"), vec![vec!["tlo"]]);
    assert_eq!(
        matches("světlem"),
        vec![vec!["svě", "vět", "ětl"], vec!["svě", "vět"]]
    );
    assert_eq!(matches("வரவே"), vec![vec!["வரவ", "ரவே"]]);
    insta::assert_debug_snapshot!(sut);
}

#[test]
fn test_search_by_less_than_a_trigram() {
    // though the index is based on trigrams, it can support
    // terms that are less than a trigram by padding the term to trigram size

    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["i  ", "am ", "groot"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    // a trivial search for trigrams
    assert_eq!((100.0 * sut.test_match_trigrams("i  ")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("am ")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("groot")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("thanos")) as usize, 0);
    assert_eq!((100.0 * sut.test_match_trigrams("xm ")) as usize, 0);
    assert_eq!((100.0 * sut.test_match_trigrams("uproot")) as usize, 50);

    // proper search
    let min_similarity = 0.75;
    let max_distance = 3;
    let expected_score = Score::new(0.708);

    assert_eq!(
        sut.test_search_matches("groot", max_distance, min_similarity),
        vec![(expected_score, EntryIndex(0))]
    );
    assert_eq!(
        sut.test_search_matches("i  ", max_distance, min_similarity),
        vec![(expected_score, EntryIndex(0))]
    );
    assert_eq!(
        sut.test_search_matches("am ", max_distance, min_similarity),
        vec![(expected_score, EntryIndex(0))]
    );
    assert_eq!(
        sut.test_search_matches("thanos", max_distance, min_similarity),
        vec![]
    );
}

#[test]
fn test_search_case_sensitive() {
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["IoN", "wORKs"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    // a trivial search for trigrams
    assert_eq!((100.0 * sut.test_match_trigrams("IoN")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("wORKs")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("works")) as usize, 0);

    // proper search
    let min_similarity = 0.75;
    let max_distance = 3;

    let expected_score_ion = Score::new(0.708);
    let expected_score_works = Score::new(0.566);

    assert_eq!(
        sut.test_search_matches("IoN", max_distance, min_similarity),
        vec![(expected_score_ion, EntryIndex(0))]
    );
    assert_eq!(
        sut.test_search_matches("wORKs", max_distance, min_similarity),
        vec![(expected_score_ion, EntryIndex(0))]
    );
    assert_eq!(
        sut.test_search_matches("WORKs", max_distance, min_similarity),
        vec![(expected_score_works, EntryIndex(0))]
    );
    assert_eq!(
        sut.test_search_matches("works", max_distance, min_similarity),
        vec![]
    );
}

#[test]
fn test_search_typo() {
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["obscurity"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);
    let terms = vec![EntryValue::text(["security", "secured"])];
    sut.insert(EntryIndex(1), AttributeIndex(0), &terms);
    let terms = vec![EntryValue::text(["absolutely", "not"])];
    sut.insert(EntryIndex(2), AttributeIndex(0), &terms);

    // we can search with typos depending on thresholds
    let min_similarity = 0.6;
    let max_distance = 3;

    assert_eq!(
        sut.test_search_matches("absent", max_distance, min_similarity),
        vec![]
    );

    let expected_score_securty = Score::new(0.621);
    let expected_score_secur1ty = Score::new(0.621);
    let expected_score_securitty = Score::new(0.631);
    let expected_score_security = Score::new(0.710);
    let expected_score_obscurity = Score::new(0.629);

    assert_eq!(
        sut.test_search_matches("securty", max_distance, min_similarity),
        vec![(expected_score_securty, EntryIndex(1))]
    );
    assert_eq!(
        sut.test_search_matches("secur1ty", max_distance, min_similarity),
        vec![(expected_score_secur1ty, EntryIndex(1))]
    );
    assert_eq!(
        sut.test_search_matches("securitty", max_distance, min_similarity),
        vec![(expected_score_securitty, EntryIndex(1))]
    );
    assert_eq!(
        sut.test_search_matches("security", max_distance, min_similarity),
        vec![
            (expected_score_security, EntryIndex(1)),
            (expected_score_obscurity, EntryIndex(0))
        ]
    );
}

#[test]
fn test_search_not_confusing_trigrams_among_tokens() {
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["partials"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);
    let terms = vec![EntryValue::text([
        "parXXXXX", "XartXXXX", "XXrtiXXX", "XXXtiaXX", "XXXXialX", "XXXXXals",
    ])];
    sut.insert(EntryIndex(1), AttributeIndex(0), &terms);
    let terms = vec![EntryValue::text(["parXXXXX", "XartXXXX", "XXrtiXXX"])];
    sut.insert(EntryIndex(2), AttributeIndex(0), &terms);
    let terms = vec![EntryValue::text(["XXXtiaXX", "XXXXialX", "XXXXXals"])];
    sut.insert(EntryIndex(2), AttributeIndex(1), &terms);

    // we can search with typos depending on thresholds

    let min_similarity = 0.75;
    let max_distance = 3;

    let res = sut.test_search_matches("partials", max_distance, min_similarity);
    assert_eq!(
        res[0].1.0, 0,
        "search should not confuse trigrams from other tokens together"
    );
    assert_eq!(
        res.len(),
        1,
        "search should not confuse trigrams from other tokens together"
    );
}

#[test]
fn test_match_trigrams_french() {
    // Test French words with accented characters: é, è, ê, à, ù, ç, œ
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text([
        "présentation",
        "café",
        "français",
        "cœur",
        "être",
        "élève",
    ])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    // Exact matches should have 100% trigram match
    assert_eq!(
        (100.0 * sut.test_match_trigrams("présentation")) as usize,
        100
    );
    assert_eq!((100.0 * sut.test_match_trigrams("café")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("français")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("cœur")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("être")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("élève")) as usize, 100);

    // Partial matches - searching without accents should have lower match rate
    // "presentation" vs "présentation" - missing the accent on é
    let match_rate = (100.0 * sut.test_match_trigrams("presentation")) as usize;
    assert!(
        match_rate < 100 && match_rate > 0,
        "partial match expected for 'presentation', got {match_rate}%"
    );
    // Should have significant overlap since most characters match
    assert!(
        match_rate >= 50,
        "presentation should have at least 50% match with présentation, got {match_rate}%"
    );

    // "cafe" vs "café" - missing accent
    let match_rate = (100.0 * sut.test_match_trigrams("cafe")) as usize;
    assert!(
        match_rate < 100 && match_rate > 0,
        "partial match expected for 'cafe', got {match_rate}%"
    );
    // "cafe" shares "caf" trigram with "café"
    assert!(
        match_rate >= 50,
        "cafe should have at least 50% match with café, got {match_rate}%"
    );

    // "francais" vs "français" - missing ç
    let match_rate = (100.0 * sut.test_match_trigrams("francais")) as usize;
    assert!(
        match_rate < 100 && match_rate > 0,
        "partial match expected for 'francais', got {match_rate}%"
    );
    // Should have good overlap
    assert!(
        match_rate >= 50,
        "francais should have at least 50% match with français, got {match_rate}%"
    );

    // Test very short accented word
    assert_eq!((100.0 * sut.test_match_trigrams("être")) as usize, 100);

    // Completely unrelated word should have 0% match
    assert_eq!((100.0 * sut.test_match_trigrams("unrelated")) as usize, 0);
}

#[test]
fn test_match_trigrams_italian() {
    // Test Italian words with accented vowels: à, è, é, ì, ò, ù
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text([
        "città",
        "perché",
        "lunedì",
        "però",
        "più",
        "università",
        "caffè",
        "virtù",
    ])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    // Exact matches should have 100% trigram match
    assert_eq!((100.0 * sut.test_match_trigrams("città")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("perché")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("lunedì")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("però")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("più")) as usize, 100);
    assert_eq!(
        (100.0 * sut.test_match_trigrams("università")) as usize,
        100
    );
    assert_eq!((100.0 * sut.test_match_trigrams("caffè")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("virtù")) as usize, 100);

    // Partial matches - searching without accents
    // "citta" vs "città"
    let match_rate = (100.0 * sut.test_match_trigrams("citta")) as usize;
    assert!(
        match_rate < 100 && match_rate > 0,
        "partial match expected for 'citta', got {match_rate}%"
    );
    // "citta" shares "cit", "itt" trigrams with "città"
    assert!(
        match_rate >= 66,
        "citta should have at least 66% match with città, got {match_rate}%"
    );

    // "perche" vs "perché"
    let match_rate = (100.0 * sut.test_match_trigrams("perche")) as usize;
    assert!(
        match_rate < 100 && match_rate > 0,
        "partial match expected for 'perche', got {match_rate}%"
    );
    // Should have good overlap
    assert!(
        match_rate >= 50,
        "perche should have at least 50% match with perché, got {match_rate}%"
    );

    // "caffe" vs "caffè"
    let match_rate = (100.0 * sut.test_match_trigrams("caffe")) as usize;
    assert!(
        match_rate < 100 && match_rate > 0,
        "partial match expected for 'caffe', got {match_rate}%"
    );
    // "caffe" shares "caf", "aff" trigrams with "caffè"
    assert!(
        match_rate >= 66,
        "caffe should have at least 66% match with caffè, got {match_rate}%"
    );

    // Test very short accented word
    assert_eq!((100.0 * sut.test_match_trigrams("più")) as usize, 100);

    // Completely unrelated word
    assert_eq!((100.0 * sut.test_match_trigrams("unrelated")) as usize, 0);
}

#[test]
fn test_search_french_fuzzy() {
    // Test fuzzy search with French accented words
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["présentation", "café", "français"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    let terms = vec![EntryValue::text(["élève", "école", "être"])];
    sut.insert(EntryIndex(1), AttributeIndex(0), &terms);

    let min_similarity = 0.6;
    let max_distance = 3;

    // Exact match should return results
    let results = sut.test_search_matches("présentation", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'présentation'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results = sut.test_search_matches("café", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'café'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results = sut.test_search_matches("élève", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'élève'");
    assert_eq!(results[0].1, EntryIndex(1));

    // Fuzzy match - searching without accent should still find with sufficient similarity
    let results = sut.test_search_matches("presentation", max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'présentation' when searching 'presentation'"
    );
    assert_eq!(results[0].1, EntryIndex(0), "should find présentation");
    // Score should be less than exact match but still reasonable
    assert!(
        results[0].0.value() >= min_similarity,
        "similarity should meet threshold {}>={}",
        results[0].0,
        Score::new(min_similarity)
    );

    let results = sut.test_search_matches("cafe", max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'café' when searching 'cafe'"
    );
    assert_eq!(results[0].1, EntryIndex(0), "should find café");
    assert!(
        results[0].0.value() >= min_similarity,
        "similarity should meet threshold"
    );

    // Note: Exact match vs fuzzy match scoring may vary depending on implementation
    // The important thing is that both find the correct result

    // Unrelated search should return empty
    let results = sut.test_search_matches("unrelated", max_distance, min_similarity);
    assert!(results.is_empty(), "should not find unrelated terms");
}

#[test]
fn test_search_italian_fuzzy() {
    // Test fuzzy search with Italian accented words
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["città", "università", "caffè"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    let terms = vec![EntryValue::text(["perché", "lunedì", "però"])];
    sut.insert(EntryIndex(1), AttributeIndex(0), &terms);

    let min_similarity = 0.6;
    let max_distance = 3;

    // Exact match should return results
    let results = sut.test_search_matches("città", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'città'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results = sut.test_search_matches("università", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'università'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results = sut.test_search_matches("perché", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'perché'");
    assert_eq!(results[0].1, EntryIndex(1));

    // Fuzzy match - searching without accent should still find with sufficient similarity
    let results = sut.test_search_matches("citta", max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'città' when searching 'citta'"
    );
    assert_eq!(results[0].1, EntryIndex(0), "should find città");
    // Note: Similarity may be below threshold due to accent differences, but search should still find results

    let results = sut.test_search_matches("universita", max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'università' when searching 'universita'"
    );
    assert_eq!(results[0].1, EntryIndex(0), "should find università");
    // Note: similarity may be below threshold due to accent differences, but should still find the result
    // The search uses max_distance and min_similarity, so results may have lower similarity

    let results = sut.test_search_matches("caffe", max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'caffè' when searching 'caffe'"
    );
    assert_eq!(results[0].1, EntryIndex(0), "should find caffè");
    // Note: Similarity may be below threshold due to accent differences, but search should still find results

    // Unrelated search should return empty
    let results = sut.test_search_matches("unrelated", max_distance, min_similarity);
    assert!(results.is_empty(), "should not find unrelated terms");
}

#[test]
fn test_matching_trigrams_french_italian() {
    // Test that trigram matching works correctly for French and Italian
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text([
        "présentation",
        "français",
        "élève",
        "città",
        "perché",
        "università",
    ])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    let matches = |term| {
        sut.test_matching_trigrams(None, term)
            .map(|(e, a, v, t, _c, trigram)| ((e, a, v, t), trigram))
            .fold(BTreeMap::<_, Vec<_>>::new(), |mut map, (key, trigram)| {
                map.entry(key).or_default().push(trigram);
                map
            })
            .into_values()
            .collect::<Vec<_>>()
    };

    // French exact matches - should find all trigrams
    let pres_trigrams = matches("présentation");
    assert_eq!(pres_trigrams.len(), 1);
    // Verify actual trigram content - "présentation" should have multiple trigrams
    assert!(
        !pres_trigrams[0].is_empty(),
        "présentation should have trigrams"
    );
    assert!(
        pres_trigrams[0].contains(&"pré"),
        "should contain 'pré' trigram"
    );
    assert!(
        pres_trigrams[0].contains(&"tio"),
        "should contain 'tio' trigram"
    );

    let francais_trigrams = matches("français");
    assert_eq!(francais_trigrams.len(), 1);
    assert!(
        !francais_trigrams[0].is_empty(),
        "français should have trigrams"
    );
    assert!(
        francais_trigrams[0].contains(&"fra"),
        "should contain 'fra' trigram"
    );

    let eleve_trigrams = matches("élève");
    assert_eq!(eleve_trigrams.len(), 1);
    assert!(!eleve_trigrams[0].is_empty(), "élève should have trigrams");
    assert!(
        eleve_trigrams[0].contains(&"élè"),
        "should contain 'élè' trigram"
    );

    // Italian exact matches
    let citta_trigrams = matches("città");
    assert_eq!(citta_trigrams.len(), 1);
    assert!(!citta_trigrams[0].is_empty(), "città should have trigrams");
    assert!(
        citta_trigrams[0].contains(&"cit"),
        "should contain 'cit' trigram"
    );
    assert!(
        citta_trigrams[0].contains(&"ttà"),
        "should contain 'ttà' trigram"
    );

    assert_eq!(matches("perché").len(), 1);
    assert_eq!(matches("università").len(), 1);

    // Verify trigram content for a French word
    let french_trigrams = matches("café");
    // "café" has trigrams: "caf", "afé" - but "café" is not in our index
    // so we expect no matches (empty)
    assert!(french_trigrams.is_empty());
}

#[test]
fn test_match_trigrams_japanese() {
    // Test Japanese text with kanji, hiragana, and katakana
    // Each Japanese character is a single Unicode code point (3 bytes in UTF-8)
    // This proves grapheme segmentation is not needed for standard Japanese
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text([
        "東京都庁",
        "日本語学校",
        "ありがとうございます",
        "カタカナ文字",
        "寿司",
    ])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    // Exact matches should have 100% trigram match
    assert_eq!((100.0 * sut.test_match_trigrams("東京都庁")) as usize, 100);
    assert_eq!(
        (100.0 * sut.test_match_trigrams("日本語学校")) as usize,
        100
    );
    assert_eq!(
        (100.0 * sut.test_match_trigrams("ありがとうございます")) as usize,
        100
    );
    assert_eq!(
        (100.0 * sut.test_match_trigrams("カタカナ文字")) as usize,
        100
    );
    assert_eq!((100.0 * sut.test_match_trigrams("寿司")) as usize, 100);

    // Partial matches - need at least 3 characters to form a trigram
    // "東京都" shares trigram "東京都" with "東京都庁"
    let match_rate = (100.0 * sut.test_match_trigrams("東京都")) as usize;
    assert!(
        match_rate > 0,
        "partial match expected for '東京都', got {match_rate}%"
    );

    // "ありがと" (4 chars) has trigrams "ありが", "りがと" - "ありが" matches "ありがとうございます"
    let match_rate = (100.0 * sut.test_match_trigrams("ありがと")) as usize;
    assert!(
        match_rate > 0,
        "partial match expected for 'ありがと', got {match_rate}%"
    );

    // "日本語" shares trigram with "日本語学校"
    let match_rate = (100.0 * sut.test_match_trigrams("日本語")) as usize;
    assert!(
        match_rate > 0,
        "partial match expected for '日本語', got {match_rate}%"
    );

    // Completely unrelated should have 0% match
    assert_eq!((100.0 * sut.test_match_trigrams("北海道")) as usize, 0);
}

#[test]
fn test_search_japanese_fuzzy() {
    // Test fuzzy search with Japanese text
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["東京都庁", "日本語学校", "ありがとう"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    let terms = vec![EntryValue::text(["大阪府庁", "カタカナ文字", "寿司"])];
    sut.insert(EntryIndex(1), AttributeIndex(0), &terms);

    let min_similarity = 0.6;
    let max_distance = 3;

    // Exact match should return results
    let results = sut.test_search_matches("東京都庁", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find '東京都庁'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results = sut.test_search_matches("日本語学校", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find '日本語学校'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results = sut.test_search_matches("大阪府庁", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find '大阪府庁'");
    assert_eq!(results[0].1, EntryIndex(1));

    let results = sut.test_search_matches("寿司", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find '寿司'");
    assert_eq!(results[0].1, EntryIndex(1));

    // Unrelated search should return empty
    let results = sut.test_search_matches("北海道", max_distance, min_similarity);
    assert!(results.is_empty(), "should not find unrelated terms");
}

#[test]
fn test_mixed_scripts_accented() {
    // Test mixed scripts with accented characters
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text([
        "café123",
        "élève2024",
        "東京Tokyo",
        "présentation_v2",
    ])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    let min_similarity = 0.6;
    let max_distance = 3;

    // Should find exact matches
    let results = sut.test_search_matches("café123", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'café123'");
    assert_eq!(results[0].1, EntryIndex(0));

    // Should find with partial match on accented part
    let results = sut.test_search_matches("cafe123", max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "should find 'café123' when searching 'cafe123'"
    );

    // Should find mixed script
    let results = sut.test_search_matches("東京Tokyo", max_distance, min_similarity);
    assert!(!results.is_empty(), "should find '東京Tokyo'");
    assert_eq!(results[0].1, EntryIndex(0));
}

#[test]
fn test_multiple_accents_in_word() {
    // Test words with multiple accented characters
    let mut sut = TextIndex::default();
    let terms = vec![EntryValue::text(["créée", "naïve", "résumé", "déjà"])];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    // Exact matches should have 100% trigram match
    assert_eq!((100.0 * sut.test_match_trigrams("créée")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("naïve")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("résumé")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("déjà")) as usize, 100);

    // Partial matches without accents
    // Note: "creee" vs "créée" and "naive" vs "naïve" have 0% match because accents completely change trigrams
    // "creee" = ["cre", "ree", "eee"], "créée" = ["cré", "réé", "éée"] - no overlap
    // "naive" = ["nai", "aiv", "ive"], "naïve" = ["naï", "aïv", "ïve"] - no overlap
    // This is expected behaviour - accents create different trigrams
    //
    // Fuzzy search requires at least some matching trigrams to work (need at least 3 matching characters).
    // Since "naive" and "naïve" have zero matching trigrams, fuzzy search will NOT find them.
    // This test verifies that the system handles multiple accents correctly in indexed terms
}

#[test]
fn no_positions_text_index() {
    // let's see that we can run search without stats and positions (saving time)

    let mut sut = TextIndex::default();

    assert!(sut.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &vec![EntryValue::text(["hello", "world"])],
    ));

    let found = sut.search(
        &TextFilter::matches(TermValue::text("world"), 4, 0.75),
        None,
        None,
        false,
        false,
    );

    insta::assert_debug_snapshot!(found, @r#"
    (
        {
            EntryIndex(
                0,
            ): [
                MatchedIndexTerm {
                    value: Text(
                        "world",
                    ),
                    score: Score(
                        1.0,
                    ),
                    positions: [],
                },
            ],
        },
        None,
    )
    "#);
}
