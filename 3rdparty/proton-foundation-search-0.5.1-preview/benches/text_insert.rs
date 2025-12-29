#![allow(clippy::expect_used)]

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use proton_foundation_search::entry::EntryValue;
use proton_foundation_search::index::prelude::{
    AttributeIndex, EntryIndex, IndexStore, IndexStoreEvent, IndexStoreOperation,
};
use proton_foundation_search::index::text::TextIndexSansIo;
use proton_foundation_search::serialization::SerDes;

/// Generates pseudo-random insert data
fn inserts() -> impl Iterator<Item = (EntryIndex, AttributeIndex, Vec<EntryValue>)> {
    let src: String = ('a'..='z').chain('A'..='Z').chain('0'..='9').collect();
    let mut map = HashMap::<(EntryIndex, AttributeIndex), Vec<Vec<(usize, Box<str>)>>>::new();
    (0usize..)
        .map(move |i| {
            let mixup = |n: usize| {
                let mut h = DefaultHasher::new();
                n.hash(&mut h);
                h.finish() as usize
            };
            let h = mixup(i);
            let e = EntryIndex((i >> 12) as u32);
            let a = AttributeIndex((i % 3) as u8);
            let term: String = (0..3 + h % 7)
                .map(|n| {
                    let n = (h * n) % src.len();
                    &src[n..n + 1]
                })
                .collect();
            (e, a, term)
        })
        .filter_map(move |(e, a, term)| {
            let mut result = None;
            if map
                .keys()
                .next()
                .map(|(current, _)| e != *current)
                .unwrap_or_default()
            {
                result = Some(std::mem::take(&mut map));
            }
            let value = map.entry((e, a)).or_insert_with(|| vec![vec![]]);

            if value.last().expect("some last").len() > 60000 {
                //add a new value if we have too many tokens
                value.push(vec![]);
            }
            let last = value.last_mut().expect("some last");
            let pos = last.len();
            last.push((pos, term.into()));
            result
        })
        .flatten()
        .map(|((e, a), tokens)| (e, a, tokens.into_iter().map(Into::into).collect()))
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut import = inserts();
    let index = TextIndexSansIo::default();
    let mut rev = 0;
    c.bench_function("insert_text", |b| {
        b.iter(|| {
            let (e, a, tokens) = import.next().expect("infinite");
            let indexed_value = Arc::new(tokens);
            for event in index.write(rev, &[IndexStoreOperation::Insert(e, a, indexed_value)]) {
                match event {
                    IndexStoreEvent::Inserted { .. } => {}
                    IndexStoreEvent::Removed { .. } => {}
                    IndexStoreEvent::Load(load_event) => {
                        (load_event.send)(&SerDes::Cbor, vec![]).expect("send")
                    }
                    IndexStoreEvent::Save(..) => {}
                    IndexStoreEvent::Release(..) => {}
                }
            }
            rev += 1;
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
