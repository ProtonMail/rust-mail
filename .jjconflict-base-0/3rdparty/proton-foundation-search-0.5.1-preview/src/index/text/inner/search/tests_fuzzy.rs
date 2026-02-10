use test_log::test;

use super::*;
#[test]
fn test_match_trigrams() {
    let mut sut = TextIndex::default();
    let terms = vec![
        ["světlo", "bungee", "வரவேற்பு"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
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
    let terms = vec![
        ["světlo", "světelný", "bungee", "வரவேற்பு", "nonsense"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
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
    let terms = vec![
        ["i  ", "am ", "groot"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
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
    let terms = vec![
        ["IoN", "wORKs"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);

    // a trivial search for trigrams
    assert_eq!((100.0 * sut.test_match_trigrams("IoN")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("wORKs")) as usize, 100);
    assert_eq!((100.0 * sut.test_match_trigrams("works")) as usize, 0);

    // proper search
    let min_similarity = 0.75;
    let max_distance = 3;

    let expected_score_ion = Score::new(0.708);
    let expected_score_works = Score::new(0.377);

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
    let terms = vec![
        ["obscurity"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);
    let terms = vec![
        ["security", "secured"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
    sut.insert(EntryIndex(1), AttributeIndex(0), &terms);
    let terms = vec![
        ["absolutely", "not"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
    sut.insert(EntryIndex(2), AttributeIndex(0), &terms);

    // we can search with typos depending on thresholds
    let min_similarity = 0.6;
    let max_distance = 3;

    assert_eq!(
        sut.test_search_matches("absent", max_distance, min_similarity),
        vec![]
    );

    let expected_score_securty = Score::new(0.373);
    let expected_score_secur1ty = Score::new(0.311);
    let expected_score_securitty = Score::new(0.451);
    let expected_score_security = Score::new(0.710);
    let expected_score_obscurity = Score::new(0.419);

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
    let terms = vec![
        ["partials"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
    sut.insert(EntryIndex(0), AttributeIndex(0), &terms);
    let terms = vec![
        [
            "parXXXXX", "XartXXXX", "XXrtiXXX", "XXXtiaXX", "XXXXialX", "XXXXXals",
        ]
        .into_iter()
        .map(Box::from)
        .enumerate()
        .collect::<Vec<_>>()
        .into(),
    ];
    sut.insert(EntryIndex(1), AttributeIndex(0), &terms);
    let terms = vec![
        ["parXXXXX", "XartXXXX", "XXrtiXXX"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
    sut.insert(EntryIndex(2), AttributeIndex(0), &terms);
    let terms = vec![
        ["XXXtiaXX", "XXXXialX", "XXXXXals"]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
    ];
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
