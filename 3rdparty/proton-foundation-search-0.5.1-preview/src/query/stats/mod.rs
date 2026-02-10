//! Search statistics and their application to entry scores

mod score;

use std::collections::{BTreeMap, HashMap};
use std::ops::{Add, AddAssign};

use serde::{Deserialize, Serialize};
use tracing::{instrument, trace};

use crate::document::Value;
#[cfg(feature = "wasm-bindgen")]
use crate::query::results::FoundEntry;

/// Search statistics for the whole collection and searched values
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct CollectionStats {
    /// Stats per attribute
    attributes: BTreeMap<Box<str>, AttributeStats>,
}

/// Search statistics for the whole collection and searched values
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct AttributeStats {
    /// How many entries exist in the searched collection in total
    entries: usize,
    /// The average entry size in the whole collection
    size: f64,
    /// Average value occurrences within the searched collection
    frequencies: BTreeMap<Value, f64>,
    /// Attribute sizes of the matched entries
    sizes: BTreeMap<Box<str>, usize>,
}

impl CollectionStats {
    /// Create new stats
    pub fn new(attributes: impl IntoIterator<Item = (Box<str>, AttributeStats)>) -> Self {
        attributes
            .into_iter()
            .map(|(attr, stats)| Self {
                attributes: [(attr, stats)].into(),
            })
            .fold(CollectionStats::default(), |mut stats, next| {
                stats += next;
                stats
            })
    }

    /// Entry sizes per attribute
    pub fn sizes(&self, entry: &str) -> impl Iterator<Item = (&str, usize)> {
        self.attributes
            .iter()
            .flat_map(|(attribute, stat)| stat.sizes.get(entry).map(|v| (attribute.as_ref(), *v)))
    }

    /// Entry attribute size
    pub fn size(&self, attribute: &str, entry: &str) -> usize {
        self.attributes
            .get(attribute)
            .and_then(|stat| stat.sizes.get(entry))
            .copied()
            .unwrap_or_default()
    }

    /// All average value occurrences within the searched collection
    pub fn frequencies(&self, attribute: &str) -> impl Iterator<Item = (&Value, f64)> {
        self.attributes
            .get(attribute)
            .into_iter()
            .flat_map(|stat| stat.frequencies.iter().map(|(k, v)| (k, *v)))
    }

    /// Average value occurrences within the searched collection
    pub fn frequency(&self, attribute: &str, value: &Value) -> f64 {
        self.attributes
            .get(attribute)
            .and_then(|stat| stat.frequencies.get(value))
            .copied()
            .unwrap_or_default()
    }

    /// Update the found entry with collection statistics
    pub fn update_all_scores<'a>(&self, found: impl IntoIterator<Item = &'a mut FoundEntry>) {
        found
            .into_iter()
            .for_each(|found| self.update_scores(found))
    }

    /// Update the found entry with collection statistics
    #[instrument]
    pub fn update_scores(&self, found: &mut FoundEntry) {
        use crate::query::results::MatchValue;

        let entry_identifier = Box::from(found.identifier());
        for MatchValue {
            value,
            score,
            occurrences,
        } in found.matches_mut()
        {
            let attr_occurrences =
                occurrences
                    .iter()
                    .fold(HashMap::new(), |mut map, occurrence| {
                        map.entry(occurrence.attribute())
                            .and_modify(|count| *count += 1_usize)
                            .or_insert(1_usize);
                        map
                    });

            *score = occurrences
                .iter()
                .filter_map(|occurrence| {
                    let AttributeStats {
                        entries,
                        size,
                        frequencies,
                        sizes,
                    } = self.attributes.get(occurrence.attribute())?;

                    let attr_freq = frequencies
                        .get(value)
                        .copied()
                        .map(|f| f / *entries as f64)?;

                    let attr_occurrences = attr_occurrences.get(occurrence.attribute()).copied()?;

                    let attr_size = sizes.get(&entry_identifier).copied()?;

                    // update score per attribute
                    // min gives most relevant first
                    let harmonized_score =
                        score.harmonize(*entries, *size, attr_freq, attr_size, attr_occurrences);

                    trace!(
                        entries,
                        size,
                        attr_freq,
                        attr_size,
                        attr_occurrences,
                        ?harmonized_score
                    );

                    Some(harmonized_score)
                })
                .min()
                .unwrap_or(*score);
        }
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl CollectionStats {
    /// Create new empty collection stats
    #[cfg(feature = "wasm-bindgen")]
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    pub fn new_wasm() -> CollectionStats {
        Default::default()
    }

    /// Create new empty collection stats
    #[cfg(feature = "wasm-bindgen")]
    #[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
    pub fn merge(&mut self, other: CollectionStats) {
        *self += other;
    }

    /// Check if the stats are empty - no entries
    pub fn is_empty(&self) -> bool {
        self.attributes.values().all(|a| a.entries == 0)
    }
}

#[cfg(feature = "wasm-bindgen")]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl CollectionStats {
    /// Number of total value occurrences within the searched collection
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "frequency")
    )]
    pub fn frequency_wasm(&self, attribute: &str, value: crate::document::wasm::Value) -> f64 {
        self.frequency(attribute, &value.into())
    }

    /// Update the found entry with collection statistics
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "updateScores")
    )]
    pub fn update_scores_wasm(&self, mut found: FoundEntry) -> FoundEntry {
        self.update_scores(&mut found);
        found
    }

    /// Update all the found entries with collection statistics
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "updateAllScores")
    )]
    pub fn update_all_scores_wasm(&self, mut found: Vec<FoundEntry>) -> Vec<FoundEntry> {
        self.update_all_scores(&mut found);
        found
    }
}

impl Add for CollectionStats {
    type Output = Self;
    fn add(mut self, rhs: Self) -> Self::Output {
        self += rhs;
        self
    }
}

impl AddAssign for CollectionStats {
    fn add_assign(&mut self, mut rhs: Self) {
        // merge attributes
        {
            // update our attributes
            for (v, ours) in &mut self.attributes {
                let other = rhs.attributes.remove(v).unwrap_or_default();
                ours.merge(other);
            }
            // include their attributes
            for (v, f) in rhs.attributes {
                self.attributes.insert(v, f);
            }
        }
    }
}

impl AttributeStats {
    /// Create new stats
    pub fn new(
        entries: usize,
        size: f64,
        frequencies: BTreeMap<Value, usize>,
        sizes: BTreeMap<Box<str>, usize>,
    ) -> Self {
        Self {
            entries,
            size,
            frequencies: frequencies
                .into_iter()
                .map(|(v, f)| (v, f as f64))
                .collect(),
            sizes,
        }
    }

    /// Merge stats together
    pub fn merge(&mut self, mut rhs: AttributeStats) {
        let total = self.entries + rhs.entries;

        // new average size
        self.size =
            (self.size * self.entries as f64 + rhs.size * rhs.entries as f64) / total as f64;

        // merge frequencies
        {
            // update our frequencies
            for (v, f) in &mut self.frequencies {
                let f2 = rhs.frequencies.remove(v).unwrap_or_default();
                *f = (*f * self.entries as f64 + f2 * rhs.entries as f64) / total as f64;
            }
            // include their frequencies
            for (v, f) in rhs.frequencies {
                self.frequencies
                    .insert(v, f * rhs.entries as f64 / total as f64);
            }
        }

        // merge entry sizes
        {
            // update our sizes
            for (v, ours) in &mut self.sizes {
                let other = rhs.sizes.remove(v).unwrap_or_default();
                *ours += other;
            }
            // include their sizes
            for (v, f) in rhs.sizes {
                self.sizes.insert(v, f);
            }
        }

        self.entries = total;
    }
}

#[test]
fn merges_attributes_for_new_stats() {
    let stats = CollectionStats::new([
        (
            "a".into(),
            AttributeStats::new(
                5,
                6.0,
                [(Value::text("v1"), 3)].into(),
                [("x".into(), 10)].into(),
            ),
        ),
        (
            "a".into(),
            AttributeStats::new(
                7,
                8.0,
                [(Value::text("v1"), 5)].into(),
                [("x".into(), 12)].into(),
            ),
        ),
    ]);
    insta::assert_debug_snapshot!(stats, @r#"
    CollectionStats {
        attributes: {
            "a": AttributeStats {
                entries: 12,
                size: 7.166666666666667,
                frequencies: {
                    Text(
                        "v1",
                    ): 4.166666666666667,
                },
                sizes: {
                    "x": 22,
                },
            },
        },
    }
    "#);
}

#[test]
fn merges_same_attrs() {
    let mut stats = CollectionStats::new([(
        "a".into(),
        AttributeStats::new(
            5,
            6.0,
            [(Value::text("v1"), 3)].into(),
            [("x".into(), 10)].into(),
        ),
    )]);
    stats += CollectionStats::new([(
        "a".into(),
        AttributeStats::new(
            7,
            8.0,
            [(Value::text("v1"), 5)].into(),
            [("x".into(), 12)].into(),
        ),
    )]);
    insta::assert_debug_snapshot!(stats, @r#"
    CollectionStats {
        attributes: {
            "a": AttributeStats {
                entries: 12,
                size: 7.166666666666667,
                frequencies: {
                    Text(
                        "v1",
                    ): 4.166666666666667,
                },
                sizes: {
                    "x": 22,
                },
            },
        },
    }
    "#);
}

#[test]
fn merges_different_attrs() {
    let mut stats = CollectionStats::new([(
        "a".into(),
        AttributeStats::new(
            5,
            6.0,
            [(Value::text("v1"), 3)].into(),
            [("x".into(), 10)].into(),
        ),
    )]);
    stats += CollectionStats::new([(
        "b".into(),
        AttributeStats::new(
            7,
            8.0,
            [(Value::text("v1"), 5)].into(),
            [("x".into(), 12)].into(),
        ),
    )]);
    insta::assert_debug_snapshot!(stats, @r#"
    CollectionStats {
        attributes: {
            "a": AttributeStats {
                entries: 5,
                size: 6.0,
                frequencies: {
                    Text(
                        "v1",
                    ): 3.0,
                },
                sizes: {
                    "x": 10,
                },
            },
            "b": AttributeStats {
                entries: 7,
                size: 8.0,
                frequencies: {
                    Text(
                        "v1",
                    ): 5.0,
                },
                sizes: {
                    "x": 12,
                },
            },
        },
    }
    "#);
}

#[test]
fn getters_empty() {
    let stats = CollectionStats::default();

    assert!(stats.is_empty());
    assert_eq!(stats.size("a", "x"), 0);
    assert_eq!(stats.frequency("a", &Value::text("v1")), 0.0);
    assert_eq!(stats.frequencies("a").count(), 0);
    assert_eq!(stats.sizes("x").count(), 0);
}

#[test]
fn getters_full() {
    let stats = CollectionStats::new([
        (
            "a".into(),
            AttributeStats::new(
                5,
                6.0,
                [(Value::text("v1"), 3)].into(),
                [("x".into(), 10)].into(),
            ),
        ),
        (
            "a".into(),
            AttributeStats::new(
                7,
                8.0,
                [(Value::text("v1"), 5)].into(),
                [("x".into(), 12)].into(),
            ),
        ),
        (
            "b".into(),
            AttributeStats::new(
                1,
                1.0,
                [(Value::text("v1"), 1)].into(),
                [("x".into(), 1)].into(),
            ),
        ),
    ]);

    assert!(!stats.is_empty());
    assert_eq!(stats.size("a", "x"), 22);
    assert_eq!(
        stats.frequency("a", &Value::text("v1")),
        (3.0 * 5.0 + 5.0 * 7.0) / 12.0
    );

    insta::assert_debug_snapshot!(stats.frequencies("a").collect::<Vec<_>>(), @r#"
    [
        (
            Text(
                "v1",
            ),
            4.166666666666667,
        ),
    ]
    "#);
    insta::assert_debug_snapshot!(stats.sizes("x").collect::<Vec<_>>(), @r#"
    [
        (
            "a",
            22,
        ),
        (
            "b",
            1,
        ),
    ]
    "#);
}

#[test]
fn scoring() {
    use crate::query::expression::Operator;
    use crate::query::results::*;

    let stats = CollectionStats::new([
        (
            "a".into(),
            AttributeStats::new(
                5,
                6.0,
                [(Value::text("v1"), 3)].into(),
                [("x".into(), 10)].into(),
            ),
        ),
        (
            "a".into(),
            AttributeStats::new(
                7,
                8.0,
                [(Value::text("v1"), 5)].into(),
                [("x".into(), 12)].into(),
            ),
        ),
        (
            "b".into(),
            AttributeStats::new(
                1,
                1.0,
                [(Value::text("v1"), 1)].into(),
                [("x".into(), 1)].into(),
            ),
        ),
    ]);

    let mut found = FoundEntry::new("x");
    stats.update_scores(&mut found);
    insta::assert_debug_snapshot!(found, @r#"
    FoundEntry {
        identifier: "x",
        matches: MatchGroup {
            operator: And,
            nodes: [],
        },
    }
    "#);

    let mut found = FoundEntry::new_with_matches(
        "x",
        MatchGroup::new(
            Operator::Or,
            [
                MatchNode::value(
                    Value::text("v1"),
                    0.5,
                    [
                        MatchOccurrence::new("a", 0, 0),
                        MatchOccurrence::new("a", 1, 0),
                    ],
                ),
                MatchNode::value(
                    Value::text("v1"),
                    0.5,
                    [
                        MatchOccurrence::new("b", 0, 0),
                        MatchOccurrence::new("no_stats_weirdo", 0, 0),
                    ],
                ),
                MatchNode::value(
                    Value::text("v1"),
                    0.5,
                    [MatchOccurrence::new("no_stats_at_all", 0, 0)],
                ),
            ],
        ),
    );
    stats.update_scores(&mut found);
    // Scores in attributes with stats (a) get updated according to term frequency * IDF.
    // Scores in attributes without stats (no_stats_at_all) remain unchanged.
    // For mix of attrs with and without stats (b with no_stats_weirdo), the applicable stats are used.
    insta::assert_debug_snapshot!(found, @r#"
    FoundEntry {
        identifier: "x",
        matches: MatchGroup {
            operator: Or,
            nodes: [
                Value(
                    MatchValue {
                        value: Text(
                            "v1",
                        ),
                        score: Score(
                            0.32568844238520994,
                        ),
                        occurrences: [
                            MatchOccurrence {
                                attribute: "a",
                                index: ValueIndex(
                                    0,
                                ),
                                position: TokenPosition(
                                    0,
                                ),
                            },
                            MatchOccurrence {
                                attribute: "a",
                                index: ValueIndex(
                                    1,
                                ),
                                position: TokenPosition(
                                    0,
                                ),
                            },
                        ],
                    },
                ),
                Value(
                    MatchValue {
                        value: Text(
                            "v1",
                        ),
                        score: Score(
                            0.3537593748197109,
                        ),
                        occurrences: [
                            MatchOccurrence {
                                attribute: "b",
                                index: ValueIndex(
                                    0,
                                ),
                                position: TokenPosition(
                                    0,
                                ),
                            },
                            MatchOccurrence {
                                attribute: "no_stats_weirdo",
                                index: ValueIndex(
                                    0,
                                ),
                                position: TokenPosition(
                                    0,
                                ),
                            },
                        ],
                    },
                ),
                Value(
                    MatchValue {
                        value: Text(
                            "v1",
                        ),
                        score: Score(
                            0.5,
                        ),
                        occurrences: [
                            MatchOccurrence {
                                attribute: "no_stats_at_all",
                                index: ValueIndex(
                                    0,
                                ),
                                position: TokenPosition(
                                    0,
                                ),
                            },
                        ],
                    },
                ),
            ],
        },
    }
    "#);
}
