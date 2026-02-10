#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use std::sync::LazyLock;

use chrono::{TimeZone, Utc};
use itertools::Itertools as _;
use proton_foundation_search::index::prelude::*;
use proton_foundation_search::index::text::TextIndexSansIo;
use proton_foundation_search::query::expression::{Func, Operator};
use proton_foundation_search::query::option::QueryOptions;
use proton_foundation_search::query::results::{
    FoundEntry, MatchGroup, MatchNode, MatchOccurrence, MatchValue, Score,
};
use proton_foundation_search::query::stats::{AttributeStats, CollectionStats};
use regex::Regex;

static WORD_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\w{3,20})").unwrap());

fn words(input: &str) -> impl Iterator<Item = String> {
    WORD_REGEX
        .captures_iter(input)
        .filter_map(|cap| cap.get(0))
        .map(|cap| cap.as_str().to_lowercase())
}

fn values(input: impl Iterator<Item = impl Into<Box<str>>>) -> EntryValues {
    input
        .batching(|iter| {
            let tokens = iter
                .take(u16::MAX as usize)
                .map(|v| v.into())
                .enumerate()
                .collect::<Vec<_>>();
            if tokens.is_empty() {
                None
            } else {
                Some(tokens.into())
            }
        })
        .collect::<Vec<_>>()
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct EmailAddress {
    pub name: String,
    pub email: String,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct Asset {
    pub id: String,
    pub subject: String,
    pub sender: EmailAddress,
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    pub bcc: Vec<EmailAddress>,
    pub time: i64,
    pub body: String,
}

impl Asset {
    pub fn load(fname: &str) -> std::io::Result<Vec<Asset>> {
        let content = std::fs::read_to_string(fname)?;
        let mut assets = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let asset: Asset = serde_json::from_str(line)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            assets.push(asset);
        }
        Ok(assets)
    }

    fn tokenize(&self) -> impl Iterator<Item = InsertEntry> {
        let Self {
            id,
            subject,
            sender,
            to,
            cc,
            bcc,
            time: _,
            body,
        } = self;
        let entry_index = id
            .split('-')
            .nth(1)
            .and_then(|s| s.parse::<u32>().ok())
            .map(EntryIndex)
            .expect("missing entry index in input");

        [
            (AttributeIndex(0), values(words(subject))),
            (AttributeIndex(1), values(words(body))),
            (
                AttributeIndex(2),
                values(words(&sender.name).chain(words(&sender.email))),
            ),
            (
                AttributeIndex(3),
                values(
                    to.iter()
                        .chain(cc.iter())
                        .chain(bcc.iter())
                        .flat_map(|addr| [&addr.name, &addr.email])
                        .flat_map(|v| words(v)),
                ),
            ),
        ]
        .into_iter()
        .map(move |(attribute_index, value)| InsertEntry {
            entry_index,
            attribute_index,
            value,
        })
    }
    #[allow(dead_code)]
    pub fn is_after(&self, other: &Asset) -> bool {
        self.time > other.time
    }
    #[allow(dead_code)]
    pub fn formatted_time(&self) -> String {
        let dt = Utc.timestamp_opt(self.time, 0).unwrap();
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

/// Entry to index in the index
#[derive(Debug)]
struct InsertEntry {
    /// Index of the entry in the collection
    entry_index: EntryIndex,
    /// Index of the attribute in the collection
    attribute_index: AttributeIndex,
    /// List of values for that entry and attribute
    value: EntryValues,
}

#[derive(Debug, Default)]
pub struct SearchIndex {
    pub inner: TextIndexSansIo,
}

impl SearchIndex {
    pub fn preload(fname: &str) -> Self {
        let inner = TextIndexSansIo::default();
        let assets = Asset::load(fname).unwrap();
        let operations = assets
            .iter()
            .flat_map(Asset::tokenize)
            .map(
                |InsertEntry {
                     entry_index,
                     attribute_index,
                     value,
                 }| {
                    IndexStoreOperation::Insert(entry_index, attribute_index, value.into())
                },
            )
            .collect::<Vec<_>>();
        for event in inner.write(0, &operations) {
            match event {
                IndexStoreEvent::Inserted { .. } => {}
                IndexStoreEvent::Removed { .. } => {}
                IndexStoreEvent::Load(load_event) => load_event.send_empty().expect("empty send"),
                IndexStoreEvent::Save(..) => {}
                IndexStoreEvent::Release(..) => {}
            }
        }

        SearchIndex { inner }
    }

    pub fn search(&self, word: &str, options: &QueryOptions) -> Vec<(Score, EntryIndex)> {
        let mut stats = CollectionStats::default();
        let mut results = self
            .inner
            .search(
                1,
                None,
                Func::Matches,
                &Value::text(word.to_lowercase()),
                options,
            )
            .into_iter()
            .flatten()
            .filter_map(|event| match event {
                IndexSearchEvent::Load(load_event) => {
                    load_event.send_empty().expect("empty send");
                    None
                }
                IndexSearchEvent::Found(entry_index, terms) => Some(FoundEntry::new_with_matches(
                    entry_index.0.to_string().into_boxed_str(),
                    MatchGroup::new(
                        Operator::Or,
                        terms.into_iter().map(|matched| {
                            MatchNode::Value(MatchValue::new(
                                matched.value,
                                matched.score,
                                matched
                                    .positions
                                    .into_iter()
                                    .map(|(a, v, p)| {
                                        MatchOccurrence::new(a.0.to_string().as_ref(), v, p)
                                    })
                                    .collect(),
                            ))
                        }),
                    ),
                )),
                IndexSearchEvent::Stats(index_stats) => {
                    stats += CollectionStats::new(index_stats.into_iter().map(
                        |(attribute, attr_stats)| {
                            (
                                attribute.0.to_string().into_boxed_str(),
                                AttributeStats::new(
                                    attr_stats.entries,
                                    attr_stats.size,
                                    attr_stats.frequencies,
                                    attr_stats
                                        .sizes
                                        .into_iter()
                                        .map(|(entry, size)| {
                                            (entry.0.to_string().into_boxed_str(), size)
                                        })
                                        .collect(),
                                ),
                            )
                        },
                    ));
                    None
                }
            })
            .collect::<Vec<_>>();

        println!("{stats:#?}");

        stats.update_all_scores(&mut results);
        results
            .into_iter()
            .map(|found| {
                (
                    found.score(),
                    found
                        .identifier()
                        .parse::<u32>()
                        .expect("reverse entry id")
                        .into(),
                )
            })
            .collect()
    }
}
