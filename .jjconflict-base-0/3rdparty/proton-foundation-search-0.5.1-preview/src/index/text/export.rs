use crate::index::prelude::{
    AttributeIndex, EntryIndex, EntryValues, IndexExport, IndexExportEvent,
};
use crate::index::text::{NAME, TextIndexSansIo};
use crate::transaction::TransactionState;

impl IndexExport for TextIndexSansIo {
    fn export(&self, revision: u64) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        let mut state = TransactionState::read(
            revision,
            NAME.into(),
            self.writer.load_full(),
            self.reader.clone(),
        );

        let mut export: Option<
            Box<dyn 'static + Send + Iterator<Item = (EntryIndex, AttributeIndex, EntryValues)>>,
        > = None;

        Box::new(std::iter::from_fn(move || {
            loop {
                if let Some(export) = &mut export {
                    let (entry, attr, value) = export.next()?;
                    return Some(IndexExportEvent::Entry { entry, attr, value });
                }

                break match state.load()? {
                    Ok(index) => {
                        // we have loaded and will just iterate the dump now
                        export = Some(Box::new((index).export()));
                        continue;
                    }
                    Err(load) => {
                        // still loading, preserve self as is
                        Some(IndexExportEvent::Load(load))
                    }
                };
            }
        }))
    }
}

#[test]
#[allow(clippy::expect_used)]
fn exports() {
    use std::sync::Arc;

    use crate::index::prelude::*;

    let sut = TextIndexSansIo::default();

    let write = sut.write(
        0,
        &[
            IndexStoreOperation::Insert(
                1.into(),
                0.into(),
                Arc::new(vec![
                    // handle overlaps
                    vec![(0, "mighty".into()), (0, "textindex".into())].into(),
                    // dump should preserve empty values
                    vec![].into(),
                    // handle token repetition
                    vec![(0, "curioser".into()), (20, "curioser".into())].into(),
                ]),
            ),
            IndexStoreOperation::Insert(
                0.into(),
                1.into(),
                Arc::new(vec![vec![(111, "lone".into())].into()]),
            ),
        ],
    );

    for event in write {
        if let crate::index::prelude::IndexStoreEvent::Load(load_event) = event {
            load_event.send_empty().expect("send")
        }
    }

    let export = sut.export(1).collect::<Vec<_>>();

    // note that the output is sorted by entry-atribute
    insta::assert_debug_snapshot!(export, @r#"
    [
        Entry {
            entry: EntryIndex(
                0,
            ),
            attr: AttributeIndex(
                1,
            ),
            value: [
                Text(
                    [
                        (
                            111,
                            "lone",
                        ),
                    ],
                ),
            ],
        },
        Entry {
            entry: EntryIndex(
                1,
            ),
            attr: AttributeIndex(
                0,
            ),
            value: [
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
                Empty,
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
            ],
        },
    ]
    "#);
}
