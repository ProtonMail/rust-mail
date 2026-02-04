mod intersection;
mod negation;
#[cfg(test)]
mod tests;
mod union;

/// Index level search event
#[derive(Debug)]
pub enum ExpressionSearchEvent {
    /// The search requires index content
    Load(LoadEvent),
    /// The search matched and scored an entry
    Found(EntryIndex, MatchNode<IndexRealm>),
    /// Search statistics
    Stats(IndexSearchStats),
}

use std::iter::empty;

use super::*;
use crate::engine::query::execution::intersection::Intersection;
use crate::engine::query::execution::negation::Negation;
use crate::engine::query::execution::union::Union;
use crate::index::prelude::{AttributeIndex, EntryIndex, IndexSearchEvent};
use crate::query::option::results::{CollectPositions, CollectStats};
use crate::query::results::IndexRealm;

impl Expression {
    /// Execute a search for this expression
    ///
    /// Multi-token AND Optimization
    ///
    /// Intersection (AND), Union (OR), Negation(!) are optimized with
    /// sorted merge which eliminates the need to build and look up in a map.
    /// Imagine ascending multiple ladders as a spider, one foot per ladder:
    ///   - AND: a rung missing in one must be skipped in all, so the top rung is the boss when all feet are on the same rung
    ///   - OR: can't finish a rung until all feet are at or above the bottom rung
    ///   - !: report the gaps in source ladder with the skipped rungs in reference ladder
    ///   - in either case, a ladder may be shorter than others and we may finish early (AND, !) or stop climbing that ladder (OR)
    pub(super) fn search(
        &self,
        attributes: &dyn Fn(&str) -> Option<AttributeIndex>,
        entries: &dyn Fn() -> VecDeque<EntryIndex>,
        indices: Vec<(BlobId, Arc<dyn Index>)>,
        options: &QueryOptions,
    ) -> Box<dyn Send + Iterator<Item = ExpressionSearchEvent>> {
        match self {
            Expression::And(expressions) if expressions.is_empty() => Box::new(core::iter::empty()),
            Expression::And(expressions) if expressions.len() == 1 => {
                expressions[0].search(attributes, entries, indices, options)
            }
            Expression::And(expressions) => Box::new(Intersection::new(
                expressions
                    .iter()
                    .map(|exp| exp.search(attributes, entries, indices.clone(), options))
                    .collect::<Vec<_>>(),
            ))
                as Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
            Expression::Or(expressions) if expressions.is_empty() => Box::new(core::iter::empty()),
            Expression::Or(expressions) if expressions.len() == 1 => {
                expressions[0].search(attributes, entries, indices, options)
            }
            Expression::Or(expressions) => Box::new(Union::new(
                expressions
                    .iter()
                    .map(|exp| exp.search(attributes, entries, indices.clone(), options))
                    .collect::<Vec<_>>(),
            ))
                as Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
            Expression::Not(expression) => {
                let options = options
                    .clone()
                    .with(|o: &mut CollectStats| *o = false.into())
                    .with(|o: &mut CollectPositions| *o = false.into());
                Box::new(Negation::new(
                    expression.search(attributes, entries, indices, &options),
                    entries,
                ))
            }
            Expression::Term {
                field,
                function,
                value,
            } => search_term(
                attributes,
                &indices,
                options,
                field.as_deref(),
                *function,
                value,
            ),
        }
    }
}

/// Execute a search for a single term expression.
fn search_term(
    attributes: &dyn Fn(&str) -> Option<AttributeIndex>,
    indices: &[(BlobId, Arc<dyn Index>)],
    options: &QueryOptions,
    field: Option<&str>,
    function: Func,
    value: &TermValue,
) -> Box<dyn Send + Iterator<Item = ExpressionSearchEvent>> {
    let attribute = match field {
        Some(field) => match attributes(field) {
            Some(attr) => Some(attr),
            None => {
                // The attribute is not present in the collection
                return Box::new(empty());
            }
        },
        None => None,
    };

    let searches = indices
        .iter()
        .filter_map(|(revision, index)| {
            let index_id:Box<str> =index.id().into();
            index
                .search(*revision, attribute, function, value, options)
                .map(|search| {
                    let mut entry_index_last = None;
                    Box::new(search.map(move |event| match event {
                        IndexSearchEvent::Load(load_event) => {
                            ExpressionSearchEvent::Load(load_event)
                        }
                        IndexSearchEvent::Found(entry_index, matched_index_terms) => {
                            if let Some(last_entry_index) = entry_index_last.replace(entry_index)
                            {
                                assert!(entry_index.0 > last_entry_index.0,
                                    "Indices must produce results ordered by EntryIndex. Index {index_id:?} had {last_entry_index}, now {entry_index}");
                            }
                            ExpressionSearchEvent::Found(
                                entry_index,
                                MatchNode::group(Operator::Or, matched_index_terms),
                            )
                        }
                        IndexSearchEvent::Stats(index_search_stats) => {
                            ExpressionSearchEvent::Stats(index_search_stats)
                        }
                    }))
                        as Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>
                })
        })
        .collect::<Vec<_>>();

    Box::new(Union::new(searches))
}
