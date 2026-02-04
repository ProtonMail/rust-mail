use proton_foundation_search::query::expression::{Expression, Func, TermValue};
use proton_foundation_search::query::option::text::{MaximumDistance, MinimumSimilarity};
use search_internal_helper as helper;
use search_internal_helper::engine_test_util::Cache;
use test_log::test;

#[test]
fn should_index_and_search() {
    let mut cache = Cache::default();
    let (engine, init) = helper::create_engine();

    cache.handle_write(init).expect("init ok");

    let query = engine
        .query()
        .with_expression("hello".parse().unwrap())
        .search();
    let found = cache.handle_search(query).expect("ok");
    assert_eq!(found.len(), 2);

    let expression = Expression::attr("creation", Func::Equals, 12345);

    let query = engine.query().with_expression(expression).search();
    let found = cache.handle_search(query).expect("ok");
    assert_eq!(found.len(), 1);

    let expression = Expression::attr("title", Func::Matches, TermValue::text("other"));

    let query = engine.query().with_expression(expression.clone()).search();
    let found = cache.handle_search(query).expect("ok");
    assert_eq!(
        found.len(),
        0,
        "trigram index search for 'other' is to different from 'another' so it will not match"
    );

    // set lower thresholds with query options
    let search = engine
        .query()
        .with_option(|o: &mut MinimumSimilarity| *o = 0.2.into())
        .with_option(|o: &mut MaximumDistance| *o = 8.into())
        .with_expression(expression.clone())
        .search();

    let found = cache.handle_search(search).expect("ok");

    assert_eq!(
        found.len(),
        1,
        "we have reduced the threshold and the term should match now"
    );
}
