use itertools::Itertools as _;
use test_log::test;

use super::*;
use crate::index::text::TextIndex as TextIndexWithBuckets;
use crate::index::text::trigram::Trigrams;
use crate::serialization::SerDes;

fn words(input: &str) -> impl Iterator<Item = String> {
    input.split_whitespace().flat_map(move |token| {
        let token = token
            .replace(|c: char| !c.is_alphanumeric(), "")
            .to_lowercase();
        let has_trigrams = token
            .trigrams()
            .next()
            .map(|(_, t)| t.len())
            .unwrap_or_default()
            >= 3;
        has_trigrams.then_some(token)
    })
}
fn values(input: &str) -> EntryValues {
    words(input)
        .batching(|iter| Some(Some(EntryValue::text(iter.take(u16::MAX as usize))?)))
        .collect::<Vec<_>>()
}

#[test]
fn test_large_bucket_index() {
    let mut cache = BTreeMap::default();
    let sut = TextIndexWithBuckets::default();
    let input = include_str!("../../../../../../../resources/english-mobydick.txt");

    let value = values(input);
    let write = sut.write(
        None,
        &[IndexStoreOperation::Insert(
            EntryIndex(0),
            AttributeIndex(0),
            value.into(),
        )],
    );
    let rev = test_util::handle_write(write, &mut cache);
    assert!(rev.is_some());

    let content: Arc<Content> = cache
        .get(&rev.expect("revision"))
        .expect("content blob")
        .downcast("content")
        .expect("content downcast");

    assert_eq!(content.vocabulary.count_trigrams(), 5246);
    assert_eq!(content.vocabulary.count_tokens(), 20006);
    assert_eq!(content.occurrences.len(), 1);

    let length = cache
        .values()
        .map(|cached| cached.serialize(&SerDes::Cbor).expect("cbor").len())
        .sum::<usize>();
    assert_eq!(length, 1603022);

    let bucket = content.occurrences[0].occurrences.id();
    let tokens: Arc<Tokens> = cache
        .get(&bucket)
        .expect("bucket")
        .downcast("tokens")
        .expect("tokens downcast");

    let mut token_counts = tokens
        .iter()
        .map(|(tref, occ)| (occ.len(), content.vocabulary.get(tref).expect("token")))
        .collect::<Vec<_>>();
    token_counts.sort();
    token_counts.reverse();
    token_counts.truncate(20);
    insta::assert_debug_snapshot!("token_counts", token_counts);

    let value = vec![EntryValue::text(words(
        "švestky zrály nejkulaťoulinkatější from hell",
    ))];
    let write = sut.write(
        rev,
        &[IndexStoreOperation::Insert(
            EntryIndex(1),
            AttributeIndex(0),
            value.into(),
        )],
    );
    let rev = test_util::handle_write(write, &mut cache);
    assert!(rev.is_some());

    let results =
        test_util::test_search_matches(&sut, "moby", rev.expect("revision"), &cache, 3, 0.75);
    insta::assert_debug_snapshot!(results,@r"
    [
        (
            Score(
                0.647,
            ),
            EntryIndex(
                0,
            ),
        ),
    ]
    ");

    let results =
        test_util::test_search_matches(&sut, "hello", rev.expect("revision"), &cache, 3, 0.75);
    insta::assert_debug_snapshot!(results,@r"
    [
        (
            Score(
                0.687,
            ),
            EntryIndex(
                0,
            ),
        ),
        (
            Score(
                0.273,
            ),
            EntryIndex(
                1,
            ),
        ),
    ]
    ");

    let results = test_util::test_search_matches(
        &sut,
        "nejkulaťounkatějš",
        rev.expect("revision"),
        &cache,
        4,
        0.75,
    );
    insta::assert_debug_snapshot!(results,@r"
    [
        (
            Score(
                1.0,
            ),
            EntryIndex(
                1,
            ),
        ),
    ]
    ");

    let results = test_util::test_search_matches(
        &sut,
        "nonexistent",
        rev.expect("revision"),
        &cache,
        3,
        0.75,
    );
    insta::assert_debug_snapshot!(results,@"[]");
}
