#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]
#![allow(unused)]

use std::collections::HashMap;
use std::sync::Arc;

use proton_foundation_search::blob::{BlobId, LoadEvent, SaveEvent};
use proton_foundation_search::document::Value;
pub use proton_foundation_search::engine::Engine;
use proton_foundation_search::engine::{Query, QueryEvent, Write, WriteEvent};
use proton_foundation_search::entry::Entry;
pub use proton_foundation_search::entry::EntryValue;
use proton_foundation_search::query::results::MatchOccurrence;
use proton_foundation_search::serialization::SerDes;
pub use test_log::test;
use tracing::debug;

pub type Storage = HashMap<BlobId, Vec<u8>>;
pub fn entry(id: &str, attrs: Vec<(&str, Vec<EntryValue>)>) -> Entry {
    Entry::new(
        id,
        attrs
            .into_iter()
            .map(|(attr, values)| {
                (
                    attr.into(),
                    Arc::new(values.into_iter().map(Some).collect()),
                )
            })
            .collect(),
    )
}
pub fn save(storage: &mut Storage, save_event: SaveEvent) {
    let id = save_event.id();
    let blob = save_event.recv();
    debug!(?id, ?blob);
    storage.insert(id, blob.serialize(&SerDes::Cbor).expect("recv"));
}
pub fn load(storage: &Storage, load_event: LoadEvent) {
    let Some(blob) = storage.get(&load_event.id()) else {
        load_event.send_empty().expect("empty send");
        return;
    };
    load_event.send(&SerDes::Cbor, blob).expect("blob send");
}
pub fn commit(storage: &mut Storage, write: Write) {
    for event in write.commit() {
        match event {
            WriteEvent::Load(load_event) => load(storage, load_event),
            WriteEvent::Save(save_event) => save(storage, save_event),
            WriteEvent::Stats(_) => {}
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn search(
    storage: &Storage,
    query: Query,
) -> Vec<(String, Vec<(Value, String, Vec<Box<str>>)>)> {
    let mut found = vec![];
    for event in query.search() {
        match event {
            QueryEvent::Load(load_event) => load(storage, load_event),
            QueryEvent::Found(found_entry) => {
                let mut matches = found_entry
                    .matches()
                    .map(|m| {
                        (
                            m.value().clone(),
                            format!("{:.3}", m.score().value()),
                            m.occurrences()
                                .into_iter()
                                .map(|o| o.attribute().into())
                                .collect(),
                        )
                    })
                    .collect::<Vec<_>>();
                matches.sort();
                found.push((found_entry.identifier().to_owned(), matches))
            }
            QueryEvent::Stats(_) => {}
        }
    }
    found.sort();
    found
}
