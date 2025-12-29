#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use std::collections::HashSet;

use proton_foundation_search::document::Value;
use proton_foundation_search::query::expression::{Expression, Func};
use search_internal_helper as helper;
use test_log::test;

use crate::storage::Storage;

#[path = "util/storage.rs"]
mod storage;

#[test]
fn should_index_and_search() {
    let mut storage = Storage::default();
    let (engine, init) = helper::create_engine();

    let modified = storage
        .handle_write(init)
        .collect::<Result<HashSet<_>, _>>()
        .expect("init ok");
    assert_eq!(modified.len(), 3);

    let query = engine
        .query()
        .with_expression("hello".parse().unwrap())
        .search();
    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");
    assert_eq!(found.len(), 2);

    let expression = Expression::attr("creation", Func::Equals, 12345);

    let query = engine.query().with_expression(expression).search();
    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");
    assert_eq!(found.len(), 1);

    let expression = Expression::attr("title", Func::Matches, Value::text("other"));

    let query = engine.query().with_expression(expression.clone()).search();
    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");
    assert_eq!(
        found.len(),
        0,
        "trigram index search for 'other' is to different from 'another' so it will not match"
    );

    // TODO: set lower thresholds with query options
    // let opts = todo!("set lower thresholds with query options");
    // let query = engine
    //     .query(&opts)
    //     .with_expression(expression.clone())
    //     .unwrap()
    //     .search();

    // let found = storage
    //     .handle_search(query)
    //     .collect::<Result<Vec<_>, _>>()
    //     .expect("ok");

    // assert_eq!(
    //     found.len(),
    //     1,
    //     "we have reduced the threshold and the term should match now"
    // );
}
