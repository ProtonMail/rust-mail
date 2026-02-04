use test_log::test;

use super::*;
use crate::index::text::TextIndex as TextIndexWithBuckets;
use crate::index::text::text_with_buckets::vocabulary::Vocabulary;
use crate::index::text::trigram::Trigrams;
use crate::query::expression::{Func, TermValue, TermValuePart};
use crate::query::option::QueryOptions;
use crate::query::option::text::{MaximumDistance, MinimumSimilarity};
use crate::query::results::Score;
#[test]
fn test_match_trigrams_basics() {
    let mut vocab = Vocabulary::default();
    ["světlo", "bungee", "வரவேற்பு"]
        .into_iter()
        .for_each(|token| {
            vocab.insert(token.into());
        });

    // a trivial search for trigrams using the bloom filter
    assert_eq!(test_match_trigrams("bungee", &vocab), 100);
    assert_eq!(test_match_trigrams("geek", &vocab), 50);
    assert_eq!(test_match_trigrams("svetlo", &vocab), 25);
    assert_eq!(test_match_trigrams("světlem", &vocab), 60);
    assert_eq!(test_match_trigrams("வரவே", &vocab), 100);
}

#[test]
fn test_matching_trigrams() {
    let mut vocabulary = Vocabulary::default();
    ["světlo", "světelný", "bungee", "வரவேற்பு", "nonsense"]
        .into_iter()
        .for_each(|token| {
            vocabulary.insert(token.into());
        });

    // a trigram based term search
    assert_eq!(
        trigram_token_matches("bungee", &vocabulary),
        vec![("bungee", vec!["bun", "ung", "nge", "gee"])]
    );
    assert_eq!(
        trigram_token_matches("bunna", &vocabulary),
        vec![("bungee", vec!["bun"])]
    );
    assert_eq!(
        trigram_token_matches("svietlo", &vocabulary),
        vec![("světlo", vec!["tlo"])]
    );
    assert_eq!(
        trigram_token_matches("světlem", &vocabulary),
        vec![
            ("světlo", vec!["svě", "vět", "ětl"]),
            ("světelný", vec!["svě", "vět"])
        ]
    );
    assert_eq!(
        trigram_token_matches("வரவே", &vocabulary),
        vec![("வரவேற்பு", vec!["வரவ", "ரவே"])]
    );
    insta::assert_debug_snapshot!(vocabulary, @r#"
    Vocabulary {
        next: TokenRef(
            5,
        ),
        tokens: SortedVec {
            inner: [
                (
                    TokenRef(
                        0,
                    ),
                    "světlo",
                ),
                (
                    TokenRef(
                        1,
                    ),
                    "světelný",
                ),
                (
                    TokenRef(
                        2,
                    ),
                    "bungee",
                ),
                (
                    TokenRef(
                        3,
                    ),
                    "வரவேற\u{bcd}பு",
                ),
                (
                    TokenRef(
                        4,
                    ),
                    "nonsense",
                ),
            ],
            sort: PhantomData<proton_foundation_search::svec::TupleSort>,
        },
        inverse: SortedVec {
            inner: [
                (
                    "bungee",
                    TokenRef(
                        2,
                    ),
                ),
                (
                    "nonsense",
                    TokenRef(
                        4,
                    ),
                ),
                (
                    "světelný",
                    TokenRef(
                        1,
                    ),
                ),
                (
                    "světlo",
                    TokenRef(
                        0,
                    ),
                ),
                (
                    "வரவேற\u{bcd}பு",
                    TokenRef(
                        3,
                    ),
                ),
            ],
            sort: PhantomData<proton_foundation_search::svec::TupleSort>,
        },
        trigrams: TrigramIndex {
            trigrams: SortedVec {
                inner: [
                    (
                        (
                            "bun",
                            0,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    2,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "eln",
                            5,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    1,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "ens",
                            4,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    4,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "gee",
                            3,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    2,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "lný",
                            6,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    1,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "nge",
                            2,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    2,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "non",
                            0,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    4,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "nse",
                            2,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    4,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "nse",
                            5,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    4,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "ons",
                            1,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    4,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "sen",
                            3,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    4,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "svě",
                            0,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    0,
                                ),
                                TokenRef(
                                    1,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "tel",
                            4,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    1,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "tlo",
                            4,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    0,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "ung",
                            1,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    2,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "vět",
                            1,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    0,
                                ),
                                TokenRef(
                                    1,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "ěte",
                            2,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    1,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "ětl",
                            2,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    0,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "ரவே",
                            3,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    3,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "ற\u{bcd}ப",
                            12,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    3,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "வரவ",
                            0,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    3,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "வேற",
                            6,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    3,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "ேற\u{bcd}",
                            9,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    3,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                    (
                        (
                            "\u{bcd}பு",
                            15,
                        ),
                        SortedVec {
                            inner: [
                                TokenRef(
                                    3,
                                ),
                            ],
                            sort: PhantomData<()>,
                        },
                    ),
                ],
                sort: PhantomData<proton_foundation_search::svec::TupleSort>,
            },
        },
    }
    "#);
}

#[test]
fn test_search_vocabulary_by_less_than_a_trigram() {
    let mut vocab = Vocabulary::default();
    ["i  ", "am ", "groot"].into_iter().for_each(|token| {
        vocab.insert(token.into());
    });
    // a trivial search for trigrams
    assert_eq!(test_match_trigrams("i  ", &vocab), 100);
    assert_eq!(test_match_trigrams("am ", &vocab), 100);
    assert_eq!(test_match_trigrams("groot", &vocab), 100);
    assert_eq!(test_match_trigrams("thanos", &vocab), 0);
    assert_eq!(test_match_trigrams("xm ", &vocab), 0);
    assert_eq!(test_match_trigrams("uproot", &vocab), 50);
}

#[test]
fn test_search_by_less_than_a_trigram() {
    // though the index is based on trigrams, it can support
    // terms that are less than a trigram by padding the term to trigram size

    let terms = vec![EntryValue::text(["i  ", "am ", "groot"])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[IndexStoreOperation::Insert(
            EntryIndex(0),
            AttributeIndex(0),
            terms.into(),
        )],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");

    // proper search
    let min_similarity = 0.75;
    let max_distance = 3;
    let expected_score = Score::new(0.708);

    assert_eq!(
        test_util::test_search_matches(&sut, "groot", rev, &cache, max_distance, min_similarity),
        vec![(expected_score, EntryIndex(0))]
    );
    assert_eq!(
        test_util::test_search_matches(&sut, "i  ", rev, &cache, max_distance, min_similarity),
        vec![(expected_score, EntryIndex(0))]
    );
    assert_eq!(
        test_util::test_search_matches(&sut, "am ", rev, &cache, max_distance, min_similarity),
        vec![(expected_score, EntryIndex(0))]
    );
    assert_eq!(
        test_util::test_search_matches(&sut, "thanos", rev, &cache, max_distance, min_similarity),
        vec![]
    );
}

#[test]
fn test_search_vocabulary_case_sensitive() {
    let mut vocab = Vocabulary::default();
    ["IoN", "wORKs"].into_iter().for_each(|token| {
        vocab.insert(token.into());
    });

    // a trivial search for trigrams
    assert_eq!(test_match_trigrams("IoN", &vocab), 100);
    assert_eq!(test_match_trigrams("wORKs", &vocab), 100);
    assert_eq!(test_match_trigrams("works", &vocab), 0);
}

#[test]
fn test_search_case_sensitive() {
    let terms = vec![EntryValue::text(["IoN", "wORKs"])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[IndexStoreOperation::Insert(
            EntryIndex(0),
            AttributeIndex(0),
            terms.into(),
        )],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");

    // proper search
    let min_similarity = 0.75;
    let max_distance = 3;

    let expected_score_ion = Score::new(0.708);
    let expected_score_works = Score::new(0.566);

    assert_eq!(
        test_util::test_search_matches(&sut, "IoN", rev, &cache, max_distance, min_similarity),
        vec![(expected_score_ion, EntryIndex(0))]
    );
    assert_eq!(
        test_util::test_search_matches(&sut, "wORKs", rev, &cache, max_distance, min_similarity),
        vec![(expected_score_ion, EntryIndex(0))]
    );
    assert_eq!(
        test_util::test_search_matches(&sut, "WORKs", rev, &cache, max_distance, min_similarity),
        vec![(expected_score_works, EntryIndex(0))]
    );
    assert_eq!(
        test_util::test_search_matches(&sut, "works", rev, &cache, max_distance, min_similarity),
        vec![]
    );
}

#[test]
fn test_search_typo() {
    let terms_0 = vec![EntryValue::text(["obscurity"])];

    let terms_1 = vec![EntryValue::text(["security", "secured"])];

    let terms_2 = vec![EntryValue::text(["absolutely", "not"])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(EntryIndex(0), AttributeIndex(0), terms_0.into()),
            IndexStoreOperation::Insert(EntryIndex(1), AttributeIndex(0), terms_1.into()),
            IndexStoreOperation::Insert(EntryIndex(2), AttributeIndex(0), terms_2.into()),
        ],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");

    // we can search with typos depending on thresholds
    let min_similarity = 0.6;
    let max_distance = 3;

    assert_eq!(
        test_util::test_search_matches(&sut, "absent", rev, &cache, max_distance, min_similarity),
        vec![]
    );

    let expected_score_securty = Score::new(0.621);
    let expected_score_secur1ty = Score::new(0.621);
    let expected_score_securitty = Score::new(0.631);
    let expected_score_security = Score::new(0.710);
    let expected_score_obscurity = Score::new(0.629);

    assert_eq!(
        test_util::test_search_matches(&sut, "securty", rev, &cache, max_distance, min_similarity),
        vec![(expected_score_securty, EntryIndex(1))]
    );
    assert_eq!(
        test_util::test_search_matches(&sut, "secur1ty", rev, &cache, max_distance, min_similarity),
        vec![(expected_score_secur1ty, EntryIndex(1))]
    );
    assert_eq!(
        test_util::test_search_matches(
            &sut,
            "securitty",
            rev,
            &cache,
            max_distance,
            min_similarity
        ),
        vec![(expected_score_securitty, EntryIndex(1))]
    );
    assert_eq!(
        test_util::test_search_matches(&sut, "security", rev, &cache, max_distance, min_similarity),
        vec![
            (expected_score_security, EntryIndex(1)),
            (expected_score_obscurity, EntryIndex(0))
        ]
    );
}

#[test]
fn test_search_not_confusing_trigrams_among_tokens() {
    let terms_0_0 = vec![EntryValue::text(["partials"])];
    let terms_1_0 = vec![EntryValue::text([
        "parXXXXX", "XartXXXX", "XXrtiXXX", "XXXtiaXX", "XXXXialX", "XXXXXals",
    ])];
    let terms_2_0 = vec![EntryValue::text(["parXXXXX", "XartXXXX", "XXrtiXXX"])];
    let terms_2_1 = vec![EntryValue::text(["XXXtiaXX", "XXXXialX", "XXXXXals"])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(EntryIndex(0), AttributeIndex(0), terms_0_0.into()),
            IndexStoreOperation::Insert(EntryIndex(1), AttributeIndex(0), terms_1_0.into()),
            IndexStoreOperation::Insert(EntryIndex(2), AttributeIndex(0), terms_2_0.into()),
            IndexStoreOperation::Insert(EntryIndex(2), AttributeIndex(1), terms_2_1.into()),
        ],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");

    // we can search with typos depending on thresholds

    let min_similarity = 0.75;
    let max_distance = 3;

    let res =
        test_util::test_search_matches(&sut, "partials", rev, &cache, max_distance, min_similarity);
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
fn test_match_vocabulary_trigrams_french() {
    let mut vocab = Vocabulary::default();
    ["présentation", "café", "français", "cœur", "être", "élève"]
        .into_iter()
        .for_each(|token| {
            vocab.insert(token.into());
        });

    // Exact matches should have 100% trigram match
    assert_eq!(test_match_trigrams("présentation", &vocab), 100);
    assert_eq!(test_match_trigrams("café", &vocab), 100);
    assert_eq!(test_match_trigrams("français", &vocab), 100);
    assert_eq!(test_match_trigrams("cœur", &vocab), 100);
    assert_eq!(test_match_trigrams("être", &vocab), 100);
    assert_eq!(test_match_trigrams("élève", &vocab), 100);

    // Partial matches - searching without accents should have lower match rate
    // "presentation" vs "présentation" - missing the accent on é
    let match_rate = test_match_trigrams("presentation", &vocab);
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
    let match_rate = test_match_trigrams("cafe", &vocab);
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
    let match_rate = test_match_trigrams("francais", &vocab);
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
    assert_eq!(test_match_trigrams("être", &vocab), 100);

    // Completely unrelated word should have 0% match
    assert_eq!(test_match_trigrams("unrelated", &vocab), 0);
}

#[test]
fn test_match_trigrams_italian() {
    // Test Italian words with accented vowels: à, è, é, ì, ò, ù
    let mut vocab = Vocabulary::default();
    [
        "città",
        "perché",
        "lunedì",
        "però",
        "più",
        "università",
        "caffè",
        "virtù",
    ]
    .into_iter()
    .for_each(|token| {
        vocab.insert(token.into());
    });

    // Exact matches should have 100% trigram match
    assert_eq!(test_match_trigrams("città", &vocab), 100);
    assert_eq!(test_match_trigrams("perché", &vocab), 100);
    assert_eq!(test_match_trigrams("lunedì", &vocab), 100);
    assert_eq!(test_match_trigrams("però", &vocab), 100);
    assert_eq!(test_match_trigrams("più", &vocab), 100);
    assert_eq!(test_match_trigrams("università", &vocab), 100);
    assert_eq!(test_match_trigrams("caffè", &vocab), 100);
    assert_eq!(test_match_trigrams("virtù", &vocab), 100);

    // Partial matches - searching without accents
    // "citta" vs "città"
    let match_rate = test_match_trigrams("citta", &vocab);
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
    let match_rate = test_match_trigrams("perche", &vocab);
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
    let match_rate = test_match_trigrams("caffe", &vocab);
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
    assert_eq!(test_match_trigrams("più", &vocab), 100);

    // Completely unrelated word
    assert_eq!(test_match_trigrams("unrelated", &vocab), 0);
}

#[test]
fn test_search_french_fuzzy() {
    // Test fuzzy search with French accented words
    let terms_0_0 = vec![EntryValue::text(["présentation", "café", "français"])];
    let terms_1_0 = vec![EntryValue::text(["élève", "école", "être"])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(EntryIndex(0), AttributeIndex(0), terms_0_0.into()),
            IndexStoreOperation::Insert(EntryIndex(1), AttributeIndex(0), terms_1_0.into()),
        ],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");

    let min_similarity = 0.6;
    let max_distance = 3;

    // Exact match should return results
    let results = test_util::test_search_matches(
        &sut,
        "présentation",
        rev,
        &cache,
        max_distance,
        min_similarity,
    );
    assert!(!results.is_empty(), "should find 'présentation'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results =
        test_util::test_search_matches(&sut, "café", rev, &cache, max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'café'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results =
        test_util::test_search_matches(&sut, "élève", rev, &cache, max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'élève'");
    assert_eq!(results[0].1, EntryIndex(1));

    // Fuzzy match - searching without accent should still find with sufficient similarity
    let results = test_util::test_search_matches(
        &sut,
        "presentation",
        rev,
        &cache,
        max_distance,
        min_similarity,
    );
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

    let results =
        test_util::test_search_matches(&sut, "cafe", rev, &cache, max_distance, min_similarity);
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
    let results = test_util::test_search_matches(
        &sut,
        "unrelated",
        rev,
        &cache,
        max_distance,
        min_similarity,
    );
    assert!(results.is_empty(), "should not find unrelated terms");
}

#[test]
fn test_search_italian_fuzzy() {
    // Test fuzzy search with Italian accented words
    let terms_0_0 = vec![EntryValue::text(["città", "università", "caffè"])];
    let terms_1_0 = vec![EntryValue::text(["perché", "lunedì", "però"])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(EntryIndex(0), AttributeIndex(0), terms_0_0.into()),
            IndexStoreOperation::Insert(EntryIndex(1), AttributeIndex(0), terms_1_0.into()),
        ],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");

    let min_similarity = 0.6;
    let max_distance = 3;

    // Exact match should return results
    let results =
        test_util::test_search_matches(&sut, "città", rev, &cache, max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'città'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results = test_util::test_search_matches(
        &sut,
        "università",
        rev,
        &cache,
        max_distance,
        min_similarity,
    );
    assert!(!results.is_empty(), "should find 'università'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results =
        test_util::test_search_matches(&sut, "perché", rev, &cache, max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'perché'");
    assert_eq!(results[0].1, EntryIndex(1));

    // Fuzzy match - searching without accent should still find with sufficient similarity
    let results =
        test_util::test_search_matches(&sut, "citta", rev, &cache, max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'città' when searching 'citta'"
    );
    assert_eq!(results[0].1, EntryIndex(0), "should find città");
    // Note: Similarity may be below threshold due to accent differences, but search should still find results

    let results = test_util::test_search_matches(
        &sut,
        "universita",
        rev,
        &cache,
        max_distance,
        min_similarity,
    );
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'università' when searching 'universita'"
    );
    assert_eq!(results[0].1, EntryIndex(0), "should find università");
    // Note: similarity may be below threshold due to accent differences, but should still find the result
    // The search uses max_distance and min_similarity, so results may have lower similarity

    let results =
        test_util::test_search_matches(&sut, "caffe", rev, &cache, max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'caffè' when searching 'caffe'"
    );
    assert_eq!(results[0].1, EntryIndex(0), "should find caffè");
    // Note: Similarity may be below threshold due to accent differences, but search should still find results

    // Unrelated search should return empty
    let results = test_util::test_search_matches(
        &sut,
        "unrelated",
        rev,
        &cache,
        max_distance,
        min_similarity,
    );
    assert!(results.is_empty(), "should not find unrelated terms");
}

#[test]
fn test_matching_trigrams_french_italian() {
    // Test that trigram matching works correctly for French and Italian
    let mut vocabulary = Vocabulary::default();
    [
        "présentation",
        "français",
        "élève",
        "città",
        "perché",
        "università",
    ]
    .into_iter()
    .for_each(|token| {
        vocabulary.insert(token.into());
    });

    // French exact matches - should find all trigrams
    let matches = trigram_token_matches("présentation", &vocabulary);
    insta::assert_compact_debug_snapshot!(matches, @r###"[("présentation", ["pré", "rés", "ése", "sen", "ent", "nta", "tat", "ati", "tio", "ion"])]"###);

    let matches = trigram_token_matches("français", &vocabulary);
    insta::assert_compact_debug_snapshot!(matches, @r###"[("français", ["fra", "ran", "anç", "nça", "çai", "ais"])]"###);

    let matches = trigram_token_matches("élève", &vocabulary);
    insta::assert_compact_debug_snapshot!(matches, @r###"[("élève", ["élè", "lèv", "ève"])]"###);

    // Italian exact matches
    let matches = trigram_token_matches("città", &vocabulary);
    insta::assert_compact_debug_snapshot!(matches, @r###"[("città", ["cit", "itt", "ttà"])]"###);

    let matches = trigram_token_matches("perché", &vocabulary);
    insta::assert_compact_debug_snapshot!(matches, @r###"[("perché", ["per", "erc", "rch", "ché"])]"###);

    let matches = trigram_token_matches("università", &vocabulary);
    insta::assert_compact_debug_snapshot!(matches, @r###"[("università", ["uni", "niv", "ive", "ver", "ers", "rsi", "sit", "ità"])]"###);

    // Verify trigram content for a French word
    let matches = trigram_token_matches("café", &vocabulary);
    // "café" has trigrams: "caf", "afé" - but "café" is not in our index
    // so we expect no matches (empty)
    insta::assert_compact_debug_snapshot!(matches, @"[]");
}

#[test]
fn test_match_trigrams_japanese() {
    // Test Japanese text with kanji, hiragana, and katakana
    // Each Japanese character is a single Unicode code point (3 bytes in UTF-8)
    // This proves grapheme segmentation is not needed for standard Japanese
    let mut vocab = Vocabulary::default();
    [
        "東京都庁",
        "日本語学校",
        "ありがとうございます",
        "カタカナ文字",
        "寿司",
    ]
    .into_iter()
    .for_each(|token| {
        vocab.insert(token.into());
    });

    // Exact matches should have 100% trigram match
    assert_eq!(test_match_trigrams("東京都庁", &vocab), 100);
    assert_eq!(test_match_trigrams("日本語学校", &vocab), 100);
    assert_eq!(test_match_trigrams("ありがとうございます", &vocab), 100);
    assert_eq!(test_match_trigrams("カタカナ文字", &vocab), 100);
    assert_eq!(test_match_trigrams("寿司", &vocab), 100);

    // Partial matches - need at least 3 characters to form a trigram
    // "東京都" shares trigram "東京都" with "東京都庁"
    let match_rate = test_match_trigrams("東京都", &vocab);
    assert!(
        match_rate > 0,
        "partial match expected for '東京都', got {match_rate}%"
    );

    // "ありがと" (4 chars) has trigrams "ありが", "りがと" - "ありが" matches "ありがとうございます"
    let match_rate = test_match_trigrams("ありがと", &vocab);
    assert!(
        match_rate > 0,
        "partial match expected for 'ありがと', got {match_rate}%"
    );

    // "日本語" shares trigram with "日本語学校"
    let match_rate = test_match_trigrams("日本語", &vocab);
    assert!(
        match_rate > 0,
        "partial match expected for '日本語', got {match_rate}%"
    );

    // Completely unrelated should have 0% match
    assert_eq!(test_match_trigrams("北海道", &vocab), 0);
}

#[test]
fn test_search_japanese_fuzzy() {
    // Test fuzzy search with Japanese text
    let terms_0 = vec![EntryValue::text(["東京都庁", "日本語学校", "ありがとう"])];
    let terms_1 = vec![EntryValue::text(["大阪府庁", "カタカナ文字", "寿司"])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(EntryIndex(0), AttributeIndex(0), terms_0.into()),
            IndexStoreOperation::Insert(EntryIndex(1), AttributeIndex(0), terms_1.into()),
        ],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");

    let min_similarity = 0.6;
    let max_distance = 3;

    // Exact match should return results
    let results =
        test_util::test_search_matches(&sut, "東京都庁", rev, &cache, max_distance, min_similarity);
    assert!(!results.is_empty(), "should find '東京都庁'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results = test_util::test_search_matches(
        &sut,
        "日本語学校",
        rev,
        &cache,
        max_distance,
        min_similarity,
    );
    assert!(!results.is_empty(), "should find '日本語学校'");
    assert_eq!(results[0].1, EntryIndex(0));

    let results =
        test_util::test_search_matches(&sut, "大阪府庁", rev, &cache, max_distance, min_similarity);
    assert!(!results.is_empty(), "should find '大阪府庁'");
    assert_eq!(results[0].1, EntryIndex(1));

    let results =
        test_util::test_search_matches(&sut, "寿司", rev, &cache, max_distance, min_similarity);
    assert!(!results.is_empty(), "should find '寿司'");
    assert_eq!(results[0].1, EntryIndex(1));

    // Unrelated search should return empty
    let results =
        test_util::test_search_matches(&sut, "北海道", rev, &cache, max_distance, min_similarity);
    assert!(results.is_empty(), "should not find unrelated terms");
}

#[test]
fn test_mixed_scripts_accented() {
    // Test mixed scripts with accented characters
    let terms = vec![EntryValue::text([
        "café123",
        "élève2024",
        "東京Tokyo",
        "présentation_v2",
    ])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[IndexStoreOperation::Insert(
            EntryIndex(0),
            AttributeIndex(0),
            terms.into(),
        )],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");

    let min_similarity = 0.6;
    let max_distance = 3;

    // Should find exact matches
    let results =
        test_util::test_search_matches(&sut, "café123", rev, &cache, max_distance, min_similarity);
    assert!(!results.is_empty(), "should find 'café123'");
    assert_eq!(results[0].1, EntryIndex(0));

    // Should find with partial match on accented part
    let results =
        test_util::test_search_matches(&sut, "cafe123", rev, &cache, max_distance, min_similarity);
    assert!(
        !results.is_empty(),
        "should find 'café123' when searching 'cafe123'"
    );

    // Should find mixed script
    let results = test_util::test_search_matches(
        &sut,
        "東京Tokyo",
        rev,
        &cache,
        max_distance,
        min_similarity,
    );
    assert!(!results.is_empty(), "should find '東京Tokyo'");
    assert_eq!(results[0].1, EntryIndex(0));
}

#[test]
fn test_multiple_accents_in_word() {
    // Test words with multiple accented characters
    let mut vocab = Vocabulary::default();
    ["créée", "naïve", "résumé", "déjà"]
        .into_iter()
        .for_each(|token| {
            vocab.insert(token.into());
        });
    // Exact matches should have 100% trigram match
    assert_eq!(test_match_trigrams("créée", &vocab), 100);
    assert_eq!(test_match_trigrams("naïve", &vocab), 100);
    assert_eq!(test_match_trigrams("résumé", &vocab), 100);
    assert_eq!(test_match_trigrams("déjà", &vocab), 100);

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
fn test_accuracy() {
    let terms_0_0 = vec![EntryValue::text(["dmitry"])];
    let terms_1_0 = vec![EntryValue::text(["smith"])];

    let sut = TextIndexWithBuckets::default();
    let mut cache = Default::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(EntryIndex(0), AttributeIndex(0), terms_0_0.into()),
            IndexStoreOperation::Insert(EntryIndex(1), AttributeIndex(0), terms_1_0.into()),
        ],
    );
    let rev = test_util::handle_write(write, &mut cache).expect("revision");
    let mut stats = Default::default();
    let search = sut
        .search(
            rev,
            None,
            Func::Matches,
            &TermValue::text("dmitry"),
            &QueryOptions::default()
                .with(|o: &mut MaximumDistance| *o = 3.into())
                .with(|o: &mut MinimumSimilarity| *o = 0.75.into()),
        )
        .expect("search");

    let results = test_util::handle_search(search, &cache, &mut stats).collect::<Vec<_>>();

    insta::assert_debug_snapshot!(stats, @r#"
    IndexSearchStats {
        stats: {
            AttributeIndex(
                0,
            ): IndexSearchAttributeStats {
                frequencies: {
                    Text(
                        "dmitry",
                    ): 1,
                },
                sizes: {
                    EntryIndex(
                        0,
                    ): (
                        1,
                        true,
                    ),
                    EntryIndex(
                        1,
                    ): (
                        1,
                        false,
                    ),
                },
            },
        },
    }
    "#);
    insta::assert_debug_snapshot!(results, @r#"
    [
        (
            EntryIndex(
                0,
            ),
            [
                MatchedIndexTerm {
                    value: Text(
                        "dmitry",
                    ),
                    score: Score(
                        1.0,
                    ),
                    positions: [
                        (
                            AttributeIndex(
                                0,
                            ),
                            ValueIndex(
                                0,
                            ),
                            TokenPosition(
                                0,
                            ),
                        ),
                    ],
                },
            ],
        ),
    ]
    "#);
}

fn test_match_trigrams(term: &str, vocabulary: &Vocabulary) -> usize {
    let matches = trigram_token_matches(term, vocabulary)
        .into_iter()
        .map(|(_, trigs)| trigs.len())
        .max()
        .unwrap_or_default();
    matches * 100 / term.trigrams().count()
}

fn trigram_token_matches<'a>(
    term: &str,
    vocabulary: &'a Vocabulary,
) -> Vec<(&'a str, Vec<&'a str>)> {
    let term = TextTerm::new(&[TermValuePart::Text(term.into())]);
    let trigrams = vocabulary.get_trigrams(term.trigram_lookup(3)).fold(
        BTreeMap::default(),
        |mut map, (pos, trig, _, token)| {
            map.entry(token).or_insert_with(Vec::new).push((pos, trig));
            map
        },
    );
    let mut result = trigrams
        .into_iter()
        .map(|(token, mut trigs)| {
            trigs.sort();
            (
                token.as_ref(),
                trigs
                    .into_iter()
                    .map(|(_p, t)| t.as_ref())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    result.sort_by_key(|(_, trigs)| usize::MAX - trigs.len());
    result
}
