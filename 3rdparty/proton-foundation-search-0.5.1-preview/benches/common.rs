#![allow(clippy::expect_used)]

use std::sync::Arc;

use proton_foundation_search::entry::EntryValue;
use proton_foundation_search::index::prelude::{
    AttributeIndex, EntryIndex, IndexStore, IndexStoreEvent, IndexStoreOperation,
};
use proton_foundation_search::serialization::SerDes;

fn words(input: &str) -> impl Iterator<Item = &str> {
    input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() < 20)
}
fn entries(input: &str) -> impl Iterator<Item = (EntryIndex, AttributeIndex, Vec<EntryValue>)> {
    #[derive(Clone, Debug, serde::Deserialize)]
    pub struct Person {
        pub name: String,
        pub email: String,
    }
    #[derive(Clone, Debug, serde::Deserialize)]
    pub struct Mail {
        //pub id: String,
        pub subject: String,
        pub body: String,
        #[serde(alias = "sender")]
        pub from: Person,
    }
    input
        .split("\n")
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(pos, line)| {
            serde_json::from_str::<Mail>(line).unwrap_or_else(|_| panic!("mail {pos} {line:?}"))
        })
        .enumerate()
        .flat_map(|(idx, mail)| {
            let Mail {
                subject,
                body,
                from,
            } = mail;
            let e = EntryIndex(idx as u32);
            [
                (
                    e,
                    AttributeIndex(0),
                    vec![
                        words(&subject)
                            .map(|s| s.into())
                            .enumerate()
                            .collect::<Vec<_>>()
                            .into(),
                    ],
                ),
                (
                    e,
                    AttributeIndex(1),
                    vec![
                        words(&body)
                            .map(|s| s.into())
                            .enumerate()
                            .collect::<Vec<_>>()
                            .into(),
                    ],
                ),
                (
                    e,
                    AttributeIndex(2),
                    vec![
                        words(&from.email)
                            .map(|s| s.into())
                            .enumerate()
                            .collect::<Vec<_>>()
                            .into(),
                        words(&from.name)
                            .map(|s| s.into())
                            .enumerate()
                            .collect::<Vec<_>>()
                            .into(),
                    ],
                ),
            ]
        })
}

pub fn import(input: &str, revision: &mut u64, index: &mut impl IndexStore) {
    let ops = entries(input)
        .map(|(e, a, v)| IndexStoreOperation::Insert(e, a, Arc::new(v)))
        .collect::<Vec<_>>();

    let write = index.write(*revision, &ops);

    for event in write {
        match event {
            IndexStoreEvent::Inserted { .. } => {
                // yay
            }
            IndexStoreEvent::Removed { .. } => unreachable!("not removing"),
            IndexStoreEvent::Load(load_event) => {
                (load_event.send)(&SerDes::Cbor, vec![]).expect("send")
            }
            IndexStoreEvent::Save(..) => {
                // ignored
            }
            IndexStoreEvent::Release(..) => {
                // ignored
            }
        }
    }
}
