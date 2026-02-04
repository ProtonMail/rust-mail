#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, VecDeque};
use std::ops::ControlFlow;

use tracing::trace;

use super::content::Content;
use super::tokens::Tokens;
use crate::blob::{Blob, BlobId};
use crate::index::prelude::{IndexExportEvent, *};
use crate::prelude::*;

pub enum Export {
    Init(BlobId),
    LoadingIndex(Blob<Content>),
    LoadingEntries(Arc<Content>, EntryAttrTokens, VecDeque<IndexExportEvent>),
}
type EntryAttrTokens = BTreeMap<EntryIndex, BTreeMap<AttributeIndex, Arc<Blob<Tokens>>>>;

impl Iterator for Export {
    type Item = IndexExportEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.progress().break_value()
    }
}

impl Export {
    pub fn new(revision: BlobId) -> Self {
        Self::Init(revision)
    }

    fn progress(&mut self) -> ControlFlow<IndexExportEvent> {
        self.init();
        self.load()?;
        self.export()
    }

    fn init(&mut self) {
        let Self::Init(revision) = self else {
            return;
        };

        *self = Export::LoadingIndex(Blob::new(*revision, "text index"));
    }

    fn load(&mut self) -> ControlFlow<IndexExportEvent> {
        let Self::LoadingIndex(blob) = self else {
            return ControlFlow::Continue(());
        };

        let content = blob.load().map_break(IndexExportEvent::Load)?;

        let mut entry_tokens = BTreeMap::new();
        let buckets = content
            .occurrences
            .iter()
            .map(|b| Arc::new(b.occurrences.clone()))
            .collect::<Vec<_>>();
        for (bucket, entries) in content.occurrences.iter().enumerate() {
            for (e, a, _) in &entries.entries {
                entry_tokens
                    .entry(*e)
                    .or_insert_with(BTreeMap::default)
                    .insert(*a, buckets[bucket].clone());
            }
        }
        // Entries must be returned in order so they can be paired with entries from other indices
        *self = Export::LoadingEntries(content.clone(), entry_tokens, Default::default());
        ControlFlow::Continue(())
    }

    fn export(&mut self) -> ControlFlow<IndexExportEvent> {
        let Export::LoadingEntries(content, entries, events) = self else {
            return ControlFlow::Continue(());
        };

        loop {
            if let Some(event) = events.pop_front() {
                return ControlFlow::Break(event);
            }

            let Some((entry, attrs)) = entries.first_key_value() else {
                return ControlFlow::Continue(());
            };

            let mut occurrences = Vec::with_capacity(attrs.len());
            for (a, bucket) in attrs {
                let tokens = bucket.load().map_break(IndexExportEvent::Load)?;
                occurrences.push((*a, tokens));
            }

            trace!(?entry, ?occurrences);

            export_entry(*entry, occurrences, content, events);

            // remove the entry just processed
            entries.pop_first();
        }
    }
}

fn export_entry(
    entry: EntryIndex,
    occurrences: Vec<(AttributeIndex, &Arc<Tokens>)>,
    content: &Arc<Content>,
    events: &mut VecDeque<IndexExportEvent>,
) {
    for (attr, occurrences) in occurrences {
        let mut value = vec![];
        for (token_ref, placements) in occurrences.iter() {
            let range = (attr, ValueIndex(0), entry, TokenPosition(0))
                ..=(
                    attr,
                    ValueIndex(usize::MAX),
                    entry,
                    TokenPosition(usize::MAX),
                );
            for (_, v, e, t) in placements.range(range) {
                if *e != entry {
                    continue;
                }
                let Some(token) = content.vocabulary.get(token_ref) else {
                    continue;
                };
                value.resize(value.len().max(v.0 + 1), vec![]);
                value[v.0].push((t.0, token.as_ref().into()));
            }
        }
        events.push_back(IndexExportEvent::Entry {
            entry,
            attr,
            value: value
                .into_iter()
                .map(|values| {
                    if values.is_empty() {
                        None
                    } else {
                        Some(EntryValue::Text(values))
                    }
                })
                .collect(),
        });
    }
}
