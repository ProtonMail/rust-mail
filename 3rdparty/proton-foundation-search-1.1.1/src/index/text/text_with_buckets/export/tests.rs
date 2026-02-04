use std::collections::BTreeMap;

use test_log::test;
use tracing::trace;

use crate::index::prelude::*;
use crate::index::text::TextIndex as TextIndexWithBuckets;
use crate::serialization::SerDes;

#[test]
fn exports() {
    let sut = TextIndexWithBuckets::default();
    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(
                1.into(),
                0.into(),
                vec![
                    // handle overlaps
                    Some(vec![(0, "mighty".into()), (0, "textindex".into())].into()),
                    // dump should preserve empty values
                    Some(vec![].into()),
                    // handle token repetition
                    Some(vec![(0, "curioser".into()), (20, "curioser".into())].into()),
                ]
                .into(),
            ),
            IndexStoreOperation::Insert(
                0.into(),
                1.into(),
                vec![Some(vec![(111, "lone".into())].into())].into(),
            ),
        ],
    );

    let mut cache = BTreeMap::new();
    let mut rev = None;
    for event in write {
        match event {
            IndexStoreEvent::Inserted { .. } | IndexStoreEvent::Removed { .. } => {}
            IndexStoreEvent::Load(load_event) => {
                load_event.send_empty().expect("empty send");
            }
            IndexStoreEvent::Save(save_event) => {
                let id = save_event.id();
                let cached = save_event.recv();
                trace!(?id);
                let _blob = cached.serialize(&SerDes::JsonPretty);
                cache.insert(id, cached);
            }
            IndexStoreEvent::Release(..) => {}
            IndexStoreEvent::Revision(revision_event) => rev = Some(revision_event.id()),
        }
    }

    let mut export = vec![];
    for event in sut.export(rev.expect("revision")) {
        match event {
            IndexExportEvent::Load(load_event) => match cache.get(&load_event.id()) {
                Some(cached) => {
                    load_event.send_cached(cached).expect("send cached");
                }
                None => {
                    load_event.send_empty().expect("send empty");
                }
            },
            IndexExportEvent::Entry { entry, attr, value } => export.push((entry, attr, value)),
        }
    }

    // note that the export is sorted by entry-attribute
    insta::assert_debug_snapshot!(export, @r#"
    [
        (
            EntryIndex(
                0,
            ),
            AttributeIndex(
                1,
            ),
            [
                Some(
                    Text(
                        [
                            (
                                111,
                                "lone",
                            ),
                        ],
                    ),
                ),
            ],
        ),
        (
            EntryIndex(
                1,
            ),
            AttributeIndex(
                0,
            ),
            [
                Some(
                    Text(
                        [
                            (
                                0,
                                "mighty",
                            ),
                            (
                                0,
                                "textindex",
                            ),
                        ],
                    ),
                ),
                None,
                Some(
                    Text(
                        [
                            (
                                0,
                                "curioser",
                            ),
                            (
                                20,
                                "curioser",
                            ),
                        ],
                    ),
                ),
            ],
        ),
    ]
    "#);
}
