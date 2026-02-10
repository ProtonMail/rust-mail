use std::collections::{BTreeMap, HashSet};

use maplit::{btreemap, hashset};
use test_log::test;

use super::*;

pub(super) type Values = EntryValues;

#[derive(Clone)]
pub(super) struct Attributes(pub(super) BTreeMap<u8, Values>);

#[derive(Clone)]
pub(super) struct Entries(pub(super) BTreeMap<u32, Attributes>);

impl Entries {
    pub(super) fn walk<F>(&self, mut f: F)
    where
        F: FnMut(EntryIndex, AttributeIndex, &EntryValues),
    {
        for (&entry_idx, entry) in &self.0 {
            let entry_idx = EntryIndex(entry_idx);
            for (&attr_idx, value) in &entry.0 {
                let attr_idx = AttributeIndex(attr_idx);
                f(entry_idx, attr_idx, value);
            }
        }
    }
}

pub(super) fn insert_index_contents(index: &mut TextIndex, contents: &Entries) {
    contents.walk(|entry_idx, attr_idx, indexed_value| {
        index.insert(entry_idx, attr_idx, indexed_value);
    });
}

pub(super) fn create_index_contents() -> Entries {
    Entries(btreemap! {
        0 => Attributes(btreemap! {
            0 => (vec![["hello", "world"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            1 => (vec![["hello", "world"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
        }),
        1 => Attributes(btreemap! {
            0 => (vec![vec! ["the", "world", "say", "hello"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
        }),
        2 => Attributes(btreemap! {
            0 => (vec![vec! ["hello"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
        }),
        3 => Attributes(btreemap! {
            1 => (vec![vec! ["just", "wanted", "say"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
        }),
    })
}

pub(super) fn create_index() -> TextIndex {
    let mut index = TextIndex::default();

    insert_index_contents(&mut index, &create_index_contents());

    index
}

#[test]
fn should_create_index() {
    let index = TextIndex::default();
    assert_eq!(index.occurrences.len(), 0);
    let index = create_index();
    assert_eq!(index.occurrences.len(), 5, "{index:#?}");
}

#[test]
fn should_insert_elements() {
    let mut index = TextIndex::default();
    let contents = create_index_contents();

    insert_index_contents(&mut index, &contents);

    contents.walk(|entry_idx, attr_idx, value| {
        for (value_idx, value) in value.iter().enumerate() {
            let EntryValue::Text(tokens) = value else {
                continue;
            };
            for (token_idx, token) in tokens {
                assert!(
                    index.test_find_posting(
                        token,
                        entry_idx,
                        attr_idx,
                        value_idx.into(),
                        (*token_idx).into()
                    ),
                    "no {token} in {index:?}"
                );
            }
        }
    });

    insta::assert_debug_snapshot!(index)
}

#[test]
fn should_move_entry() {
    let mut source = TextIndex::default();

    insert_index_contents(
        &mut source,
        &Entries(btreemap! {
            0 => Attributes(btreemap! {
                0 => (vec![["hello", "world"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
            1 => Attributes(btreemap! {
                0 => (vec![["hello", "alice", "hello", "bob"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
            2 => Attributes(btreemap! {
                0 => (vec![["hello", "eve"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
        }),
    );

    assert_eq!(
        source.test_get_terms().collect::<HashSet<_>>(),
        hashset![
            "hello".into(),
            "world".into(),
            "alice".into(),
            "bob".into(),
            "eve".into()
        ]
    );
    assert_eq!(
        source.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![EntryIndex(0), EntryIndex(1), EntryIndex(2)]
    );

    let removed = source.remove(&[EntryIndex(0)].into());
    assert_eq!(
        removed,
        vec![(
            EntryIndex(0),
            AttributeIndex(0),
            (vec![
                ["hello", "world"]
                    .into_iter()
                    .map(Box::from)
                    .enumerate()
                    .collect::<Vec<_>>()
                    .into()
            ])
        )]
    );
    assert_eq!(
        source.test_get_terms().collect::<HashSet<_>>(),
        hashset!["hello".into(), "alice".into(), "bob".into(), "eve".into()]
    );
    assert_eq!(
        source.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![EntryIndex(1), EntryIndex(2)]
    );

    let removed = source.remove(&[EntryIndex(1)].into());
    assert_eq!(
        removed,
        vec![(
            EntryIndex(1),
            AttributeIndex(0),
            vec![
                ["hello", "alice", "hello", "bob"]
                    .into_iter()
                    .map(Box::from)
                    .enumerate()
                    .collect::<Vec<_>>()
                    .into()
            ]
        )]
    );
    assert_eq!(
        source.test_get_terms().collect::<HashSet<_>>(),
        hashset!["hello".into(), "eve".into()]
    );
    assert_eq!(
        source.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![EntryIndex(2)]
    );

    let removed = source.remove(&[EntryIndex(2)].into());
    assert_eq!(
        removed,
        vec![(
            EntryIndex(2),
            AttributeIndex(0),
            (vec![
                ["hello", "eve"]
                    .into_iter()
                    .map(Box::from)
                    .enumerate()
                    .collect::<Vec<_>>()
                    .into()
            ])
        )]
    );
    assert_eq!(source.test_get_terms().collect::<HashSet<_>>(), hashset![]);
    assert_eq!(
        source.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![]
    );
}

#[test]
fn should_remove_entry() {
    let mut index = TextIndex::default();

    insert_index_contents(
        &mut index,
        &Entries(btreemap! {
            0 => Attributes(btreemap! {
                0 => (vec![["hello", "world"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
            1 => Attributes(btreemap! {
                0 => (vec![["hello", "alice", "hello", "bob"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
            2 => Attributes(btreemap! {
                0 => (vec![["hello", "eve"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
        }),
    );

    assert_eq!(
        index.test_get_terms().collect::<HashSet<_>>(),
        hashset![
            "hello".into(),
            "world".into(),
            "alice".into(),
            "bob".into(),
            "eve".into()
        ]
    );
    assert_eq!(
        index.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![EntryIndex(0), EntryIndex(1), EntryIndex(2)]
    );

    let _ = index.remove(&[EntryIndex(0)].into());
    assert_eq!(
        index.test_get_terms().collect::<HashSet<_>>(),
        hashset!["hello".into(), "alice".into(), "bob".into(), "eve".into()]
    );
    assert_eq!(
        index.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![EntryIndex(1), EntryIndex(2)]
    );

    let _ = index.remove(&[EntryIndex(1)].into());
    assert_eq!(
        index.test_get_terms().collect::<HashSet<_>>(),
        hashset!["hello".into(), "eve".into()]
    );
    assert_eq!(
        index.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![EntryIndex(2)]
    );

    let _ = index.remove(&[EntryIndex(2)].into());
    assert_eq!(index.test_get_terms().collect::<HashSet<_>>(), hashset![]);
    assert_eq!(
        index.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![]
    );
}

#[test]
fn should_remap_entries() {
    let mut index = TextIndex::default();
    let mapping = {
        let mut res = BTreeMap::new();
        res.insert(EntryIndex(5), EntryIndex(3));
        res
    };
    assert!(!index.remap(&mapping));

    insert_index_contents(
        &mut index,
        &Entries(btreemap! {
            0 => Attributes(btreemap! {
                0 => (vec![["just", "wanted", "say"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
            2 => Attributes(btreemap! {
                0 => (vec![["whatever", "the", "content"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
            4 => Attributes(btreemap! {
                0 => (vec![["this", "cool"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
            }),
        }),
    );

    let mapping = {
        let mut res = BTreeMap::new();
        res.insert(EntryIndex(4), EntryIndex(1));
        res
    };
    assert!(index.remap(&mapping), "entry ID should be changed");
    assert!(
        index.test_entry_has_term(EntryIndex(1), "this"),
        "{index:#?}"
    );
    assert!(index.test_entry_has_term(EntryIndex(1), "cool"),);
    assert_eq!(
        index.test_get_terms().collect::<HashSet<_>>(),
        hashset![
            "just".into(),
            "wanted".into(),
            "say".into(),
            "whatever".into(),
            "the".into(),
            "content".into(),
            "this".into(),
            "cool".into(),
        ]
    );
    assert_eq!(
        index.test_get_all_entry_ids().collect::<HashSet<_>>(),
        hashset![EntryIndex(0), EntryIndex(2), EntryIndex(1)]
    );
}

#[test]
fn test_content_size() {
    let mut index = TextIndex::default();
    let content_size = index.stats().length();
    assert_eq!(content_size, 0,);
    assert!(index.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &(vec![["one"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
    ));
    let content_size = {
        let new = index.stats().length();
        assert_eq!(new - content_size, 3);
        new
    };

    index.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &(vec![
            ["one", "two"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );
    let content_size = {
        let new = index.stats().length();
        assert_eq!(new - content_size, 3);
        new
    };

    index.insert(
        EntryIndex(1),
        AttributeIndex(1),
        &(vec![
            ["one", "xxx"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );
    let content_size = {
        let new = index.stats().length();
        assert_eq!(new - content_size, 6);
        new
    };

    index.insert(
        EntryIndex(1),
        AttributeIndex(1),
        &(vec![
            ["one"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
            ["xxx", "yyy"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );
    let content_size = {
        let new = index.stats().length();
        assert_eq!(new - content_size, 3);
        new
    };

    index.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &(vec![
            ["one", "two"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
            ["xxx", "yyy"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );
    let content_size = {
        let new = index.stats().length();
        assert_eq!(
            new - content_size,
            12,
            "adding the same token to different entries should have a smaller footprint"
        );
        new
    };

    index.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &(vec![
            ["one", "one", "two"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
            ["xxx", "yyy"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );
    let content_size = {
        let new = index.stats().length();
        assert_eq!(
            new,
            3 + content_size,
            "adding the same token to the same entry multiple times should have the least footprint {index:#?}"
        );
        new
    };

    index.insert(EntryIndex(1), AttributeIndex(0), &EntryValues::default());
    let content_size = {
        let new = index.stats().length();
        assert_eq!(
            new,
            content_size - 15,
            "must shrink on value removals {index:#?}"
        );
        new
    };

    assert!(!index.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &(vec![["one", "two"].into_iter().map(Box::from).enumerate().collect::<Vec<_>>().into()]),
    ),);
    let _content_size = {
        let new = index.stats().length();
        assert_eq!(new - content_size, 0, "no duplicates");
        new
    };

    insta::assert_debug_snapshot!(index);
}

#[test]
fn should_bulk_move_entries_simple_example() {
    use super::*;

    let mut index = TextIndex::default();

    // Insert 5 entries with different tokens
    // Entry 0: "hello world"
    index.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &(vec![
            ["hello", "world"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );

    // Entry 1: "hello there"
    index.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &(vec![
            ["hello", "there"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );

    // Entry 2: "foo bar"
    index.insert(
        EntryIndex(2),
        AttributeIndex(0),
        &(vec![
            ["foo", "bar"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );

    // Entry 3: "baz qux"
    index.insert(
        EntryIndex(3),
        AttributeIndex(0),
        &(vec![
            ["baz", "qux"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );

    // Entry 4: "hello foo"
    index.insert(
        EntryIndex(4),
        AttributeIndex(0),
        &(vec![
            ["hello", "foo"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ]),
    );

    // Verify initial state
    assert_eq!(index.occurrences.len(), 5);
    tracing::info!("Initial token count = {}", index.tokens.len());
    // We have: "hello", "world", "there", "foo", "bar", "baz", "qux" = 7 tokens

    // Move entries 1 and 3
    let moved = index.remove(&[EntryIndex(1), EntryIndex(3)].into());

    // Debug: Print what was actually moved
    tracing::info!("moved = {:?}", moved);
    for (entry_idx, attr_idx, indexed_value) in &moved {
        tracing::info!(
            "entry={:?}, attr={:?}, tokens={:?}",
            entry_idx,
            attr_idx,
            indexed_value
        );
    }

    // Verify moved data - should have 2 entries
    assert_eq!(moved.len(), 2);

    // Find entry 1's data (should have "hello", "there")
    let entry1_data = moved
        .iter()
        .find(|(entry_idx, _, indexed_value)| {
            let EntryValue::Text(text) = &indexed_value[0] else {
                return false;
            };
            *entry_idx == EntryIndex(1)
                && text
                    .iter()
                    .map(|(_, token)| token.as_ref())
                    .contains("there")
        })
        .unwrap();

    let tokens1 = &entry1_data.2[0];
    tracing::info!("tokens1 = {:?}", tokens1);
    insta::assert_debug_snapshot!(tokens1, @r#"
    Text(
        [
            (
                0,
                "hello",
            ),
            (
                1,
                "there",
            ),
        ],
    )
    "#);

    // Find entry 3's data (should have "baz", "qux")
    let entry3_data = moved
        .iter()
        .find(|(entry_idx, _, indexed_value)| {
            let EntryValue::Text(text) = &indexed_value[0] else {
                return false;
            };
            *entry_idx == EntryIndex(3)
                && text.iter().map(|(_, token)| token.as_ref()).contains("baz")
        })
        .unwrap();

    let tokens3 = &entry3_data.2[0];
    tracing::info!("tokens3 = {:?}", tokens3);
    insta::assert_debug_snapshot!(tokens3, @r#"
    Text(
        [
            (
                0,
                "baz",
            ),
            (
                1,
                "qux",
            ),
        ],
    )
    "#);

    // Verify final state - entries 0, 2, 4 remain
    assert_eq!(index.occurrences.len(), 3);
    tracing::info!("Final token count = {}", index.tokens.len());
    assert_eq!(index.tokens.len(), 4); // "hello", "foo", "world", "bar" (others removed)

    // Collect remaining token strings
    let remaining_tokens: Vec<_> = index
        .tokens
        .iter()
        .map(|(token, _)| token.as_ref())
        .collect();
    tracing::info!("Remaining tokens = {:?}", remaining_tokens);
    assert!(remaining_tokens.contains(&"hello"));
    assert!(remaining_tokens.contains(&"foo"));
    assert!(remaining_tokens.contains(&"world"));
    assert!(remaining_tokens.contains(&"bar"));
    assert_eq!(remaining_tokens.len(), 4);

    // Verify "hello" still has occurrences for entries 0 and 4
    let hello_token_ref = TokenRef::new(0); // First token
    let hello_occurrences = index.token_occurrences(hello_token_ref);
    assert_eq!(hello_occurrences.len(), 2); // Entries 0 and 4 remain
}

#[test]
fn should_bulk_move_entries_with_five_sentences() {
    use super::*;

    let mut index = TextIndex::default();

    // Insert the five sentences as separate entries
    // Sentence 1: "How many roads must a man walk down."
    index.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &vec![
            ["how", "many", "roads", "must", "man", "walk", "down"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );

    // Sentence 2: "Too many people have died."
    index.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &vec![
            ["too", "many", "people", "have", "died"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );

    // Sentence 3: "I'm walkin' down that long lonesome road."
    index.insert(
        EntryIndex(2),
        AttributeIndex(0),
        &vec![
            ["walkin", "down", "that", "long", "lonesome", "road"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );

    // Sentence 4: "You don't need a weatherman to know which way the wind blows."
    index.insert(
        EntryIndex(3),
        AttributeIndex(0),
        &vec![
            [
                "you",
                "don",
                "need",
                "weatherman",
                "know",
                "which",
                "way",
                "wind",
                "blows",
            ]
            .into_iter()
            .map(Box::from)
            .enumerate()
            .collect::<Vec<_>>()
            .into(),
        ],
    );

    // Sentence 5: "Death is not the end."
    index.insert(
        EntryIndex(4),
        AttributeIndex(0),
        &vec![
            ["death", "not", "end"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );

    // Verify initial state
    assert_eq!(index.occurrences.len(), 5);
    tracing::info!("Initial token count = {}", index.tokens.len());

    // Expected shared tokens:
    // - "many" appears in entries 0 and 1
    // - "down" appears in entries 0 and 2

    // Move entries 1 and 3 (sentences 2 and 4)
    let moved = index.remove(&[EntryIndex(1), EntryIndex(3)].into());

    // Debug: Print what was actually moved
    tracing::info!("moved = {:?}", moved);
    for (entry_idx, attr_idx, indexed_value) in &moved {
        tracing::info!(
            "entry={:?}, attr={:?},  tokens={:?}",
            entry_idx,
            attr_idx,
            indexed_value
        );
    }

    // Verify moved data - should have 2 entries
    assert_eq!(moved.len(), 2);

    // Find entry 1's data (should have "too", "many", "people", "have", "died")
    let entry1_data = moved
        .iter()
        .find(|(entry_idx, _, indexed_value)| {
            let EntryValue::Text(text) = &indexed_value[0] else {
                return false;
            };
            *entry_idx == EntryIndex(1)
                && text.iter().map(|(_, token)| token.as_ref()).contains("too")
        })
        .unwrap();

    let tokens1 = &entry1_data.2[0];
    tracing::info!("tokens1 = {:?}", tokens1);
    insta::assert_debug_snapshot!(tokens1, @r#"
    Text(
        [
            (
                0,
                "too",
            ),
            (
                1,
                "many",
            ),
            (
                2,
                "people",
            ),
            (
                3,
                "have",
            ),
            (
                4,
                "died",
            ),
        ],
    )
    "#);

    // Find entry 3's data (should have "you", "don", "need", "weatherman", "know", "which", "way", "wind", "blows")
    let entry3_data = moved
        .iter()
        .find(|(entry_idx, _, indexed_value)| {
            let EntryValue::Text(text) = &indexed_value[0] else {
                return false;
            };
            *entry_idx == EntryIndex(3)
                && text
                    .iter()
                    .map(|(_, token)| token.as_ref())
                    .contains("weatherman")
        })
        .unwrap();

    let tokens3 = &entry3_data.2[0];
    tracing::info!("tokens3 = {:?}", tokens3);
    insta::assert_debug_snapshot!(tokens3, @r#"
    Text(
        [
            (
                0,
                "you",
            ),
            (
                1,
                "don",
            ),
            (
                2,
                "need",
            ),
            (
                3,
                "weatherman",
            ),
            (
                4,
                "know",
            ),
            (
                5,
                "which",
            ),
            (
                6,
                "way",
            ),
            (
                7,
                "wind",
            ),
            (
                8,
                "blows",
            ),
        ],
    )
    "#);
    // Verify final state - entries 0, 2, 4 remain
    assert_eq!(index.occurrences.len(), 3);
    tracing::info!("Final token count = {}", index.tokens.len());

    // Collect remaining token strings
    let remaining_tokens: Vec<_> = index
        .tokens
        .iter()
        .map(|(token, _)| token.as_ref())
        .collect();
    tracing::info!("Remaining tokens = {:?}", remaining_tokens);

    // Verify shared tokens still exist
    assert!(remaining_tokens.contains(&"many")); // Shared between entries 0 and 1 (entry 1 was moved)
    assert!(remaining_tokens.contains(&"down")); // Shared between entries 0 and 2 (both remain)

    // Verify unique tokens from remaining entries
    assert!(remaining_tokens.contains(&"how"));
    assert!(remaining_tokens.contains(&"roads"));
    assert!(remaining_tokens.contains(&"must"));
    assert!(remaining_tokens.contains(&"man"));
    assert!(remaining_tokens.contains(&"walk"));
    assert!(remaining_tokens.contains(&"walkin"));
    assert!(remaining_tokens.contains(&"that"));
    assert!(remaining_tokens.contains(&"long"));
    assert!(remaining_tokens.contains(&"lonesome"));
    assert!(remaining_tokens.contains(&"road"));
    assert!(remaining_tokens.contains(&"death"));
    assert!(remaining_tokens.contains(&"not"));
    assert!(remaining_tokens.contains(&"end"));

    // Verify tokens from moved entries are gone
    assert!(!remaining_tokens.contains(&"too"));
    assert!(!remaining_tokens.contains(&"people"));
    assert!(!remaining_tokens.contains(&"have"));
    assert!(!remaining_tokens.contains(&"died"));
    assert!(!remaining_tokens.contains(&"you"));
    assert!(!remaining_tokens.contains(&"don"));
    assert!(!remaining_tokens.contains(&"need"));
    assert!(!remaining_tokens.contains(&"weatherman"));
    assert!(!remaining_tokens.contains(&"know"));
    assert!(!remaining_tokens.contains(&"which"));
    assert!(!remaining_tokens.contains(&"way"));
    assert!(!remaining_tokens.contains(&"wind"));
    assert!(!remaining_tokens.contains(&"blows"));

    // Verify "many" still has occurrences for entry 0 (entry 1 was moved)
    let many_token_ref = remaining_tokens
        .iter()
        .position(|&t| t == "many")
        .map(|pos| TokenRef::new(pos as u32))
        .unwrap();
    let many_occurrences = index.token_occurrences(many_token_ref);
    assert_eq!(many_occurrences.len(), 1); // Only entry 0 remains

    // Verify "down" still has occurrences for entries 0 and 2 (both remain)
    let down_token_ref = remaining_tokens
        .iter()
        .position(|&t| t == "down")
        .map(|pos| TokenRef::new(pos as u32))
        .unwrap();
    let down_occurrences = index.token_occurrences(down_token_ref);
    assert_eq!(down_occurrences.len(), 2); // Entries 0 and 2 remain
}

#[test]
fn should_bulk_move_entries_with_correct_mapping() {
    use super::*;

    let mut index = TextIndex::default();

    // Insert 3 entries with distinct content
    index.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &vec![
            ["entry0_token1", "entry0_token2"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );

    index.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &vec![
            ["entry1_token1", "entry1_token2"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );

    index.insert(
        EntryIndex(2),
        AttributeIndex(0),
        &vec![
            ["entry2_token1", "entry2_token2"]
                .into_iter()
                .map(Box::from)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );

    // Verify initial state
    assert_eq!(index.occurrences.len(), 3);

    // Move entries 0 and 1 (keeping entry 2)
    let moved = index.remove(&[EntryIndex(0), EntryIndex(1)].into());

    // Verify that the moved data includes the EntryIndex for proper mapping
    tracing::info!("moved = {:?}", moved);

    // Verify we have moved data for 2 entries
    assert_eq!(moved.len(), 2, "Should have moved data for 2 entries");

    // Verify that each moved entry has the correct EntryIndex association
    let entry0_data = moved
        .iter()
        .find(|(entry_idx, _, _)| *entry_idx == EntryIndex(0))
        .unwrap();
    let entry1_data = moved
        .iter()
        .find(|(entry_idx, _, _)| *entry_idx == EntryIndex(1))
        .unwrap();

    // Verify entry 0's data
    let tokens0 = &entry0_data.2[0];
    insta::assert_debug_snapshot!(tokens0, @r#"
    Text(
        [
            (
                0,
                "entry0_token1",
            ),
            (
                1,
                "entry0_token2",
            ),
        ],
    )
    "#);

    // Verify entry 1's data
    let tokens1 = &entry1_data.2[0];
    insta::assert_debug_snapshot!(tokens1, @r#"
    Text(
        [
            (
                0,
                "entry1_token1",
            ),
            (
                1,
                "entry1_token2",
            ),
        ],
    )
    "#);

    // Verify final state - only entry 2 remains
    assert_eq!(index.occurrences.len(), 1);
}
