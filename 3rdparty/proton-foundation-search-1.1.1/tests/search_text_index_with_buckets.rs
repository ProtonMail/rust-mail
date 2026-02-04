use std::collections::BTreeMap;

use proton_foundation_search::index::prelude::*;
use proton_foundation_search::index::text::TextIndex as TextIndexWithBuckets;
use proton_foundation_search::query::expression::{Func, TermValue};
use proton_foundation_search::query::option::QueryOptions;
use search_internal_helper::index_test_util::{do_test_commit, do_test_load, format_storage};
use test_log::test;

#[test]
fn loading() {
    let mut storage = BTreeMap::new();
    let mut cache = BTreeMap::new();
    let sut = TextIndexWithBuckets::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(
                EntryIndex(12),
                AttributeIndex(23),
                vec![EntryValue::text(["good", "morning"])].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(21),
                AttributeIndex(32),
                vec![EntryValue::text(["foot", "morfing", "ants"])].into(),
            ),
        ],
    );
    let revision = do_test_commit(write, &mut storage, &mut cache).expect("revision");

    insta::assert_snapshot!(format_storage(&storage));
    assert!(storage.contains_key(&revision));
    assert!(cache.contains_key(&revision));

    let mut events = String::new();
    let term = TermValue::text("food");
    for event in sut
        .search(
            revision,
            None,
            Func::Matches,
            &term,
            &QueryOptions::default(),
        )
        .expect("search")
    {
        match event {
            IndexSearchEvent::Found(entry_index, _matches) => {
                events += &format!("found {entry_index:?}\n")
            }
            IndexSearchEvent::Load(load_event) => {
                events += &format!("load {:?}\n", load_event);
                do_test_load(load_event, &mut storage, &mut cache);
            }
            IndexSearchEvent::Stats(_index_search_stats) => {}
        }
    }

    insta::assert_snapshot!(events, @r#"
    load LoadEvent { name: D95AB4E923C550BBB7576C5AADDC0EE5, description: "text index", .. }
    load LoadEvent { name: 22DEC48C4B57567C83C7E82B3B9833EA, description: "token occurrences bucket 0", .. }
    found EntryIndex(12)
    found EntryIndex(21)
    "#);
}
