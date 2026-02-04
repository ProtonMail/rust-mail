use alloc::collections::VecDeque;
use alloc::sync::Arc;

use tracing::{error, trace};

use super::content::Content;
use super::tokens::{TokenOccurrences, Tokens};
use super::vocabulary::TokenRef;
use super::*;
use crate::blob::{Blob, BlobId, ReleaseEvent, RevisionEvent};
use crate::index::text::text_with_buckets::content::Bucket;

#[derive(Debug)]
pub struct Executor {
    index: TextIndex,
    staging: Staging,
    blob: Option<Blob<Content>>,
    stage: Stage,
}

#[derive(Debug)]
enum Stage {
    Init,
    Loaded {
        content: Content,
    },
    Removing {
        content: Content,
        token_bucket_removals_to_process: Vec<(usize, Vec<(AttributeIndex, EntryIndex)>)>,
        token_buckets_updated: BTreeMap<usize, Tokens>,
        entry_tokens_removed: BTreeMap<usize, BTreeMap<TokenRef, TokenOccurrences>>,
    },
    Inserting {
        content: Content,
        token_buckets_updated: BTreeMap<usize, Tokens>,
        entry_tokens_removed: BTreeMap<usize, BTreeMap<TokenRef, TokenOccurrences>>,
        entry_tokens_inserted: BTreeMap<TokenRef, TokenOccurrences>,
        /// Which bucket will receive the tokens?
        insert_bucket: Option<Tokens>,
        /// Which bucket number will receive the tokens?
        insert_bucket_number: usize,
    },
    Saving {
        content: Content,
        token_buckets_updated: BTreeMap<usize, Tokens>,
        modified: bool,
        events: VecDeque<IndexStoreEvent>,
        removed_tokens: BTreeSet<TokenRef>,
    },
    Finishing {
        events: VecDeque<IndexStoreEvent>,
    },
    Done,
    Transient,
}

impl Iterator for Executor {
    type Item = IndexStoreEvent;

    fn next(&mut self) -> Option<Self::Item> {
        match self.proceed() {
            ControlFlow::Break(event) => Some(event),
            ControlFlow::Continue(()) => None,
        }
    }
}

impl Executor {
    pub fn new(write: Write) -> Self {
        let Write { id, staging, index } = write;

        Self {
            blob: id.map(|id| Blob::new(id, "text index")),
            staging,
            index,
            stage: Stage::Init,
        }
    }

    #[instrument]
    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn proceed(&mut self) -> ControlFlow<IndexStoreEvent> {
        trace!("proceed");
        self.init().map_break(IndexStoreEvent::Load)?;
        self.remove().map_break(IndexStoreEvent::Load)?;
        self.insert().map_break(IndexStoreEvent::Load)?;
        self.save()?;
        self.stage = self.verify();
        ControlFlow::Continue(())
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn init(&mut self) -> ControlFlow<LoadEvent> {
        let Stage::Init = self.stage else {
            return ControlFlow::Continue(());
        };

        if self.staging.inserts.is_empty() && self.staging.removals.is_empty() {
            self.stage = Stage::Done;
            return ControlFlow::Continue(());
        }

        let content = if let Some(blob) = &mut self.blob {
            blob.fetch_and_free()?.as_ref().clone()
        } else {
            Default::default()
        };

        self.stage = Stage::Loaded { content };
        ControlFlow::Continue(())
    }

    /// Check equilibrium after processing all changes
    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn verify(&self) -> Stage {
        let Self {
            staging,
            blob,
            stage,
            index: _,
        } = self;

        match stage {
            Stage::Done => {
                trace!("No work left to do, that's good at this stage.");
            }
            Stage::Finishing { events } if events.is_empty() => {
                trace!("No more events to finish.");
            }
            _ => {
                error!("Cannot still be working {self:#?}");
                debug_assert!(false, "Cannot still be working {self:#?}");
            }
        }

        debug_assert!(staging.inserts.is_empty());
        debug_assert!(staging.removals.is_empty());
        debug_assert!(blob.as_ref().map(Blob::is_free).unwrap_or(true));
        Stage::Done
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn save(&mut self) -> ControlFlow<IndexStoreEvent> {
        self.stage = self.start_saving();
        self.save_tokens();
        self.stage = self.save_manifest();
        self.save_events()?;
        ControlFlow::Continue(())
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn start_saving(&mut self) -> Stage {
        let stage = core::mem::replace(&mut self.stage, Stage::Transient);
        let Stage::Inserting {
            content,
            mut token_buckets_updated,
            insert_bucket,
            insert_bucket_number,
            entry_tokens_removed,
            entry_tokens_inserted,
        } = stage
        else {
            return stage;
        };

        if let Some(insert_bucket) = insert_bucket
            && !insert_bucket.is_empty()
        {
            // push the insert bucket to be processed with the removals
            // if there are any changes (inserted!=removed)
            if entry_tokens_removed.get(&insert_bucket_number) != Some(&entry_tokens_inserted) {
                let removed = token_buckets_updated.insert(insert_bucket_number, insert_bucket);
                debug_assert!(removed.is_none());
            }
        }

        let removed_tokens = entry_tokens_removed
            .iter()
            .flat_map(|(_, t)| t.keys())
            .filter(|tref| !entry_tokens_inserted.contains_key(tref))
            .copied()
            .collect();

        Stage::Saving {
            content,
            modified: false,
            token_buckets_updated,
            events: Default::default(),
            removed_tokens,
        }
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn save_tokens(&mut self) {
        let Stage::Saving {
            content,
            modified,
            events,
            token_buckets_updated,
            removed_tokens: _,
        } = &mut self.stage
        else {
            return;
        };

        let mut removed_buckets = 0;
        for (bucket_number, tokens) in core::mem::take(token_buckets_updated) {
            *modified = true;

            let bucket_number = bucket_number - removed_buckets;

            if tokens.is_empty() {
                if bucket_number < content.occurrences.len() {
                    let removed = content.occurrences.remove(bucket_number);
                    debug_assert!(removed.entries.is_empty());
                    events.push_back(IndexStoreEvent::Release(ReleaseEvent::new(
                        removed.occurrences.id(),
                    )));
                }
                removed_buckets += 1;
                continue;
            }

            let id = BlobId::generate(&tokens);
            let description = format!("token occurrences bucket {}", bucket_number);

            let token_refs = tokens.keys().copied().collect();
            let entries = tokens
                .values()
                .flatten()
                .map(|(a, _, e, _)| (*e, *a))
                .fold(BTreeMap::new(), |mut map, (e, a)| {
                    *map.entry((e, a)).or_default() += 1;
                    map
                })
                .into_iter()
                .map(|((e, a), size)| (e, a, size))
                .collect();

            if bucket_number == content.occurrences.len() {
                content.occurrences.push(Bucket {
                    entries,
                    tokens: token_refs,
                    occurrences: Blob::new(id, description),
                });
            } else if bucket_number > content.occurrences.len() {
                unreachable!(
                    "invalid token bucket index: want {bucket_number}, have {}",
                    content.occurrences.len()
                );
            } else {
                content.occurrences[bucket_number].entries = entries;
                content.occurrences[bucket_number].tokens = token_refs;
            }

            let blob = &mut content.occurrences[bucket_number].occurrences;

            let (release, save) = blob.save_and_free(id, Arc::new(tokens));
            events.push_back(IndexStoreEvent::Save(save));
            events.extend(release.map(IndexStoreEvent::Release));
        }
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn save_manifest(&mut self) -> Stage {
        let stage = core::mem::replace(&mut self.stage, Stage::Transient);
        let Stage::Saving {
            mut content,
            modified,
            mut events,
            token_buckets_updated,
            removed_tokens,
        } = stage
        else {
            return stage;
        };

        debug_assert!(
            token_buckets_updated.is_empty(),
            "token_buckets_updated is not empty {self:#?}"
        );

        if !modified {
            debug_assert!(
                token_buckets_updated.is_empty(),
                "token_buckets_updated is not empty {self:#?}"
            );
            return Stage::Finishing { events };
        }

        // remove unused tokens
        for tref in removed_tokens {
            if !content
                .occurrences
                .iter()
                .any(|b| b.tokens.contains_key(&tref))
            {
                content.vocabulary.remove(&tref);
            }
        }

        let id = BlobId::generate(&content);
        let blob = self.blob.get_or_insert_with(|| Blob::new(id, "text index"));

        let (release, save) = blob.save_and_free(id, Arc::new(content));

        let rev = RevisionEvent::new(save.id());
        events.push_back(IndexStoreEvent::Save(save));
        events.push_back(IndexStoreEvent::Revision(rev));
        events.extend(release.map(IndexStoreEvent::Release));

        debug_assert!(
            token_buckets_updated.is_empty(),
            "token_buckets_updated is not empty {self:#?}"
        );

        Stage::Finishing { events }
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn save_events(&mut self) -> ControlFlow<IndexStoreEvent> {
        let Stage::Finishing { events } = &mut self.stage else {
            return ControlFlow::Continue(());
        };

        trace!("events: {events:#?}");
        if let Some(event) = events.pop_front() {
            return ControlFlow::Break(event);
        };
        ControlFlow::Continue(())
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn remove(&mut self) -> ControlFlow<LoadEvent> {
        self.stage = self.start_removing();
        self.remove_tokens()
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn start_removing(&mut self) -> Stage {
        let stage = core::mem::replace(&mut self.stage, Stage::Transient);
        let Stage::Loaded { mut content } = stage else {
            return stage;
        };

        let token_bucket_removals_to_process = if self.staging.removals.is_empty() {
            Default::default()
        } else {
            Self::prepare_removals(&mut content, core::mem::take(&mut self.staging.removals))
        };

        Stage::Removing {
            content,
            token_bucket_removals_to_process,
            entry_tokens_removed: Default::default(),
            token_buckets_updated: Default::default(),
        }
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn prepare_removals(
        content: &mut Content,
        removals: Vec<(EntryIndex, AttributeIndex, AttributeIndex)>,
    ) -> Vec<(usize, Vec<(AttributeIndex, EntryIndex)>)> {
        let mut removed = vec![];
        for (bucket_num, bucket) in content.occurrences.iter_mut().enumerate() {
            let mut removed_entries = vec![];
            for (e, attr_start, attr_end) in &removals {
                for (e, a, _size) in bucket
                    .entries
                    .remove_range((*e, *attr_start, 0)..=(*e, *attr_end, usize::MAX))
                {
                    trace!("Removed {e:?} {a:?} {_size:?} from {bucket_num}");
                    removed_entries.push((a, e));
                }
            }
            if !removed_entries.is_empty() {
                removed.push((bucket_num, removed_entries));
            }
        }
        removed
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn remove_tokens(&mut self) -> ControlFlow<LoadEvent> {
        let Stage::Removing {
            content,
            token_bucket_removals_to_process,
            entry_tokens_removed,
            token_buckets_updated,
        } = &mut self.stage
        else {
            return ControlFlow::Continue(());
        };

        while let Some((token_bucket_number_to_process, removed_entries)) =
            token_bucket_removals_to_process.last()
        {
            trace!(
                "loading bucket for removal: {token_bucket_number_to_process} from {token_bucket_removals_to_process:#?}"
            );
            #[allow(clippy::expect_used)]
            let bucket = content
                .fetch_tokens(*token_bucket_number_to_process)?
                .expect("bucket must exist");

            let tokens = Self::do_remove_tokens(
                &bucket,
                removed_entries,
                entry_tokens_removed
                    .entry(*token_bucket_number_to_process)
                    .or_default(),
            );

            token_buckets_updated.insert(*token_bucket_number_to_process, tokens);

            // once done with the batch, go to next one
            token_bucket_removals_to_process.pop();
        }

        ControlFlow::Continue(())
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn do_remove_tokens(
        bucket: &Tokens,
        removed_entries: &[(AttributeIndex, EntryIndex)],
        entry_tokens_removed: &mut BTreeMap<TokenRef, TokenOccurrences>,
    ) -> Tokens {
        // filter out the removals, recording tokens removed
        bucket
            .iter()
            .filter_map(|(token, positions)| {
                let positions = positions
                    .iter()
                    .filter_map(|(attr, value_index, entry, token_position)| {
                        if removed_entries.contains(&(*attr, *entry)) {
                            entry_tokens_removed.entry(*token).or_default().insert((
                                *attr,
                                *value_index,
                                *entry,
                                *token_position,
                            ));
                            return None;
                        }
                        Some((*attr, *value_index, *entry, *token_position))
                    })
                    .collect::<BTreeSet<_>>();
                if positions.is_empty() {
                    return None;
                }
                Some((*token, positions))
            })
            .collect::<BTreeMap<_, _>>()
            .into()
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn insert(&mut self) -> ControlFlow<LoadEvent> {
        self.stage = self.start_inserting();
        self.insert_tokens()?;
        ControlFlow::Continue(())
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn start_inserting(&mut self) -> Stage {
        let stage = core::mem::replace(&mut self.stage, Stage::Transient);
        let Stage::Removing {
            mut content,
            token_bucket_removals_to_process,
            entry_tokens_removed,
            mut token_buckets_updated,
        } = stage
        else {
            return stage;
        };

        debug_assert!(token_bucket_removals_to_process.is_empty());

        let (insert_bucket_number, insert_bucket) = if self.staging.inserts.is_empty() {
            (content.occurrences.len(), None)
        } else {
            Self::pick_insert_token_bucket(&mut content, self.index.maximum_token_bucket_size)
                .map(|i| (i, token_buckets_updated.remove(&i)))
                .unwrap_or_else(|| {
                    let i = content.occurrences.len();
                    (i, Some(Tokens::default()))
                })
        };

        Stage::Inserting {
            content,
            entry_tokens_removed,
            entry_tokens_inserted: Default::default(),
            insert_bucket,
            insert_bucket_number,
            token_buckets_updated,
        }
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn pick_insert_token_bucket(
        content: &mut Content,
        maximum_token_bucket_size: usize,
    ) -> Option<usize> {
        if maximum_token_bucket_size == 0 {
            // shortcut the size calculation, always new bucket
            return None;
        }

        // Check the most recent bucket for size.
        // If too big, we'll create a new one
        let bucket = content.occurrences.last()?;
        {
            let size: usize = bucket.entries.iter().map(|(_, _, size)| *size).sum();

            if size >= maximum_token_bucket_size {
                None
            } else {
                Some(content.occurrences.len().saturating_sub(1))
            }
        }
    }

    #[cfg_attr(feature = "bench-mode", inline(never))]
    fn insert_tokens(&mut self) -> ControlFlow<LoadEvent> {
        if self.staging.inserts.is_empty() {
            return ControlFlow::Continue(());
        }

        let Stage::Inserting {
            content,
            insert_bucket,
            insert_bucket_number,
            token_buckets_updated: _,
            entry_tokens_inserted,
            entry_tokens_removed: _,
        } = &mut self.stage
        else {
            return ControlFlow::Continue(());
        };

        let insert_bucket = match insert_bucket {
            Some(b) => b,
            None => {
                trace!("loading bucket {insert_bucket_number}");
                insert_bucket.insert(
                    content
                        .fetch_tokens(*insert_bucket_number)?
                        .map(|b| b.as_ref().clone())
                        .unwrap_or_default(),
                )
            }
        };

        for ((attr, entry), values) in core::mem::take(&mut self.staging.inserts) {
            for (value_index, value) in values.iter().enumerate() {
                let value_index = ValueIndex(value_index);
                let Some(EntryValue::Text(tokens)) = value else {
                    continue;
                };
                for (token_position, token) in tokens {
                    let token_position = TokenPosition(*token_position);
                    let token = content.vocabulary.insert(token.into());

                    insert_bucket.entry(token).or_default().insert((
                        attr,
                        value_index,
                        entry,
                        token_position,
                    ));

                    entry_tokens_inserted.entry(token).or_default().insert((
                        attr,
                        value_index,
                        entry,
                        token_position,
                    ));
                }
            }
        }

        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use super::*;
    use crate::blob::Cached;
    use crate::index::text::text_with_buckets::vocabulary::Vocabulary;
    use crate::serialization::SerDes;
    #[test]
    fn removes_empty_buckets() {
        let mut sut = crate::index::text::text_with_buckets::write::Executor {
            staging: Staging {
                removals: vec![(EntryIndex(1), AttributeIndex(0), AttributeIndex(0))],
                inserts: [].into(),
            },
            blob: Some(Blob::new(BlobId::new(0, 0), "test index")),
            stage: Stage::Init,
            index: TextIndex::default(),
        };

        let mut vocabulary = Vocabulary::default();
        let tref = vocabulary.insert("word".into());

        match sut.next() {
            Some(IndexStoreEvent::Load(load)) => {
                load.send_cached(&Cached::new(
                    "test index",
                    Arc::new(Content {
                        vocabulary,
                        occurrences: vec![
                            Bucket {
                                entries: vec![(EntryIndex(1), AttributeIndex(0), 1)].into(),
                                tokens: vec![tref].into(),
                                occurrences: Blob::new(BlobId::new(1, 0), "b10"),
                            },
                            Bucket {
                                entries: vec![(EntryIndex(1), AttributeIndex(1), 1)].into(),
                                tokens: vec![tref].into(),
                                occurrences: Blob::new(BlobId::new(1, 1), "b11"),
                            },
                        ],
                    }),
                ))
                .expect("send cached");
            }
            unexpected => panic!("unexpected {unexpected:?}"),
        }

        match sut.next() {
            Some(IndexStoreEvent::Load(load)) if load.id() == BlobId::new(1, 0) => {
                load.send_cached(&Cached::new(
                    "tokens 1-0",
                    Arc::new(Tokens::new(
                        [(
                            tref,
                            [(
                                AttributeIndex(0),
                                ValueIndex(0),
                                EntryIndex(1),
                                TokenPosition(0),
                            )]
                            .into(),
                        )]
                        .into(),
                    )),
                ))
                .expect("send cached");
            }
            unexpected => panic!("unexpected {unexpected:?}"),
        }

        match sut.next() {
            Some(IndexStoreEvent::Release(release)) if release.id() == BlobId::new(1, 0) => {}
            unexpected => panic!("unexpected {unexpected:?}"),
        };

        let save = match sut.next() {
            Some(IndexStoreEvent::Save(save)) => save,
            unexpected => panic!("unexpected {unexpected:?}"),
        };
        let content: Arc<Content> = save.recv().downcast("dummy").expect("downcast");

        assert_eq!(
            content.occurrences.len(),
            1,
            "only one bucket should remain in {content:#?} \n{sut:#?}"
        );
    }

    #[test]
    fn skips_with_no_text() {
        let mut sut = crate::index::text::text_with_buckets::write::Executor {
            staging: Staging {
                removals: [].into(),
                inserts: [(
                    (AttributeIndex(0), EntryIndex(0)),
                    vec![EntryValue::new(1), EntryValue::new("x")].into(),
                )]
                .into(),
            },
            blob: None,
            stage: Stage::Init,
            index: TextIndex::default(),
        };

        match sut.next() {
            None => {}
            unexpected => panic!("unexpected {unexpected:?}"),
        }
    }

    #[test]
    fn skips_non_text_attrs() {
        let mut sut = crate::index::text::text_with_buckets::write::Executor {
            staging: Staging {
                removals: [].into(),
                inserts: [
                    (
                        (AttributeIndex(0), EntryIndex(0)),
                        vec![EntryValue::new(1), EntryValue::new("x")].into(),
                    ),
                    (
                        (AttributeIndex(0), EntryIndex(1)),
                        vec![EntryValue::text(["x"])].into(),
                    ),
                ]
                .into(),
            },
            blob: None,
            stage: Stage::Init,
            index: TextIndex::default(),
        };

        match sut.next() {
            Some(IndexStoreEvent::Save(event)) => {
                let bucket =
                    String::from_utf8(event.recv().serialize(&SerDes::JsonPretty).expect("json"))
                        .expect("json str");
                insta::assert_snapshot!("bucket", bucket)
            }
            unexpected => panic!("unexpected {unexpected:?}"),
        }

        match sut.next() {
            Some(IndexStoreEvent::Save(event)) => {
                let content =
                    String::from_utf8(event.recv().serialize(&SerDes::JsonPretty).expect("json"))
                        .expect("json str");
                insta::assert_snapshot!("content", content)
            }
            unexpected => panic!("unexpected {unexpected:?}"),
        }

        match sut.next() {
            Some(IndexStoreEvent::Revision(_)) => {}
            unexpected => panic!("unexpected {unexpected:?}"),
        }

        match sut.next() {
            None => {}
            unexpected => panic!("unexpected {unexpected:?}"),
        }
    }
}
