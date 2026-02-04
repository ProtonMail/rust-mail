use test_log::test;

use super::*;
use crate::index::text::text_with_buckets::content::Content;
use crate::serialization::SerDes;

#[test]
fn test_defragmentation() {
    let mut cache = BTreeMap::new();
    let sut = TextIndex::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(
                EntryIndex(0),
                AttributeIndex(0),
                vec![Some(
                    ["defragmentation"]
                        .into_iter()
                        .map(Box::from)
                        .enumerate()
                        .collect::<Vec<_>>()
                        .into(),
                )]
                .into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(1),
                AttributeIndex(0),
                vec![Some(
                    ["calcidation"]
                        .into_iter()
                        .map(Box::from)
                        .enumerate()
                        .collect::<Vec<_>>()
                        .into(),
                )]
                .into(),
            ),
        ],
    );

    let rev = test_util::handle_write(write, &mut cache);

    let vocab: Arc<Content> = cache
        .get(&rev.expect("revision"))
        .expect("content blob")
        .downcast("content")
        .expect("downcast content");
    assert_eq!(vocab.vocabulary.count_tokens(), 2);
    assert_eq!(vocab.vocabulary.count_trigrams(), 19);
    assert_eq!(vocab.occurrences.len(), 1);
    assert_eq!(vocab.occurrences[0].entries.len(), 2);
    assert_eq!(vocab.occurrences[0].tokens.len(), 2);

    let dump = cache
        .values()
        .flat_map(|cached| {
            [
                cached.to_string(),
                "\n".to_owned(),
                String::from_utf8(cached.serialize(&SerDes::JsonPretty).expect("json"))
                    .expect("string"),
                "\n".to_owned(),
            ]
        })
        .collect::<String>();
    insta::assert_snapshot!(dump);

    let write = sut.write(rev, &[IndexStoreOperation::Remove(EntryIndex(0))]);
    let rev = test_util::handle_write(write, &mut cache);

    let vocab: Arc<Content> = cache
        .get(&rev.expect("revision"))
        .expect("content blob")
        .downcast("content")
        .expect("downcast content");
    assert_eq!(vocab.vocabulary.count_tokens(), 1);
    assert_eq!(vocab.vocabulary.count_trigrams(), 9);
    assert_eq!(vocab.occurrences.len(), 1);
    assert_eq!(vocab.occurrences[0].entries.len(), 1);
    assert_eq!(vocab.occurrences[0].tokens.len(), 1);

    let dump = cache
        .values()
        .flat_map(|cached| {
            [
                cached.to_string(),
                "\n".to_owned(),
                String::from_utf8(cached.serialize(&SerDes::JsonPretty).expect("json"))
                    .expect("string"),
                "\n".to_owned(),
            ]
        })
        .collect::<String>();
    insta::assert_snapshot!(dump);
}

#[test]
fn test_token_removal_from_multiple_buckets() {
    let mut cache = BTreeMap::new();
    let sut = TextIndex::default();
    let mut rev = None;
    let write = sut.write(
        rev,
        &[IndexStoreOperation::Insert(
            EntryIndex(0),
            AttributeIndex(0),
            vec![EntryValue::text(["word"])].into(),
        )],
    );

    rev = test_util::handle_write(write, &mut cache);
    let write = sut.write(
        rev,
        &[IndexStoreOperation::Insert(
            EntryIndex(1),
            AttributeIndex(0),
            vec![
                // same token: word
                EntryValue::text(["word"]),
            ]
            .into(),
        )],
    );

    let rev = test_util::handle_write(write, &mut cache);

    let vocab: Arc<Content> = cache
        .get(&rev.expect("revision"))
        .expect("content blob")
        .downcast("content")
        .expect("downcast content");
    assert_eq!(vocab.vocabulary.count_tokens(), 1);
    assert_eq!(vocab.vocabulary.count_trigrams(), 2);
    assert_eq!(vocab.occurrences.len(), 2);
    assert_eq!(vocab.occurrences[0].entries.len(), 1);
    assert_eq!(vocab.occurrences[0].tokens.len(), 1);
    assert_eq!(vocab.occurrences[1].entries.len(), 1);
    assert_eq!(vocab.occurrences[1].tokens.len(), 1);

    let dump = cache
        .iter()
        .flat_map(|(id, cached)| {
            [
                id.to_string(),
                "\n".to_owned(),
                cached.to_string(),
                "\n".to_owned(),
                String::from_utf8(cached.serialize(&SerDes::JsonPretty).expect("json"))
                    .expect("string"),
                "\n".to_owned(),
            ]
        })
        .collect::<String>();
    insta::assert_snapshot!(dump);

    let write = sut.write(rev, &[IndexStoreOperation::Remove(EntryIndex(0))]);
    let rev = test_util::handle_write(write, &mut cache);

    let vocab: Arc<Content> = cache
        .get(&rev.expect("revision"))
        .expect("content blob")
        .downcast("content")
        .expect("downcast content");
    assert_eq!(vocab.vocabulary.count_tokens(), 1);
    assert_eq!(vocab.vocabulary.count_trigrams(), 2);
    assert_eq!(vocab.occurrences.len(), 1);
    assert_eq!(vocab.occurrences[0].entries.len(), 1);
    assert_eq!(vocab.occurrences[0].tokens.len(), 1);

    let dump = cache
        .iter()
        .flat_map(|(id, cached)| {
            [
                id.to_string(),
                "\n".to_owned(),
                cached.to_string(),
                "\n".to_owned(),
                String::from_utf8(cached.serialize(&SerDes::JsonPretty).expect("json"))
                    .expect("string"),
                "\n".to_owned(),
            ]
        })
        .collect::<String>();
    insta::assert_snapshot!(dump);
}
