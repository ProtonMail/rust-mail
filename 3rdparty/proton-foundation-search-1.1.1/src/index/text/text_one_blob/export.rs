use std::ops::ControlFlow;

use super::inner::TextIndex;
use super::{NAME, TextIndexSansIo};
use crate::blob::{Blob, BlobId};
use crate::index::prelude::{
    AttributeIndex, EntryIndex, EntryValues, IndexExport, IndexExportEvent,
};

impl IndexExport for TextIndexSansIo {
    fn export(
        &self,
        revision: BlobId,
    ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        let mut state = Blob::<TextIndex>::new(revision, NAME);

        let mut export: Option<
            Box<dyn 'static + Send + Iterator<Item = (EntryIndex, AttributeIndex, EntryValues)>>,
        > = None;

        Box::new(std::iter::from_fn(move || {
            loop {
                if let Some(export) = &mut export {
                    let (entry, attr, value) = export.next()?;
                    return Some(IndexExportEvent::Entry { entry, attr, value });
                }

                break match state.fetch_and_free() {
                    ControlFlow::Continue(index) => {
                        // we have loaded and will just iterate the dump now
                        export = Some(Box::new((index).export()));
                        continue;
                    }
                    ControlFlow::Break(load) => {
                        // still loading, preserve self as is
                        Some(IndexExportEvent::Load(load))
                    }
                };
            }
        }))
    }
}

#[test]
fn exports() {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use crate::index::prelude::*;
    use crate::serialization::SerDes;

    let sut = TextIndexSansIo::default();

    let write = sut.write(
        None,
        &[
            IndexStoreOperation::Insert(
                1.into(),
                0.into(),
                Arc::new(vec![
                    // handle overlaps
                    Some(vec![(0, "mighty".into()), (0, "textindex".into())].into()),
                    // dump should preserve empty values
                    Some(vec![].into()),
                    // handle token repetition
                    Some(vec![(0, "curioser".into()), (20, "curioser".into())].into()),
                ]),
            ),
            IndexStoreOperation::Insert(
                0.into(),
                1.into(),
                Arc::new(vec![Some(vec![(111, "lone".into())].into())]),
            ),
        ],
    );

    let mut storage = BTreeMap::new();
    let mut revision = None;

    for event in write {
        match event {
            IndexStoreEvent::Inserted { .. } | IndexStoreEvent::Removed { .. } => {}
            IndexStoreEvent::Load(load_event) => {
                load_event.send_empty().expect("send");
            }
            IndexStoreEvent::Release(_) => {
                unreachable!("no blobs should be released")
            }
            IndexStoreEvent::Revision(revision_event) => revision = Some(revision_event.id()),
            IndexStoreEvent::Save(save_event) => {
                storage.insert(
                    save_event.id(),
                    save_event.recv().serialize(&SerDes::Json).expect("recv"),
                );
            }
        }
    }

    let export = sut
        .export(revision.expect("revision"))
        .filter_map(|event| match event {
            IndexExportEvent::Load(load_event) => {
                if let Some(data) = storage.get(&load_event.id()) {
                    load_event.send(&SerDes::Json, data).expect("send");
                    None
                } else {
                    unreachable!("must not ask for absent blobs");
                }
            }
            IndexExportEvent::Entry { entry, attr, value } => Some((entry, attr, value)),
        })
        .collect::<Vec<_>>();

    // note that the output is sorted by entry-atribute
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
