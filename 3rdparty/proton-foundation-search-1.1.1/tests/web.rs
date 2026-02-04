//! Test suite for the Web and headless browsers.

#![cfg(all(target_family = "wasm", feature = "wasm-bindgen"))]

use std::collections::HashMap;

use js_sys::Array;
use proton_foundation_search::document::Document;
use proton_foundation_search::document::wasm::Value;
use proton_foundation_search::engine::Engine;
use proton_foundation_search::engine::wasm::{QueryEventKind, TextToken, WriteEventKind};
use proton_foundation_search::entry::wasm::EntryValue;
use proton_foundation_search::query::expression::wasm::Expression;
use proton_foundation_search::query::expression::{Func, TermValue};
use proton_foundation_search::query::option::QueryOptions;
use tracing::info;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
async fn should_populate_index() {
    proton_foundation_search::setup::enable_tracing();
    let title = "title";
    let creation = "creation";

    let engine = Engine::builder().build();

    let mut writer = engine.write().unwrap();
    let mut doc = Document::new("foo.txt");
    doc.add_attribute_value(title, Value::text("Hello World"));
    doc.add_attribute_value(creation, Value::int(12345));
    writer.insert(doc);

    let mut doc = Document::new("bar.txt");
    doc.add_attribute_value(title.into(), Value::text("Hello Another World"));
    doc.add_attribute_value(creation.into(), Value::int(12356));
    writer.insert(doc);

    let mut cache = HashMap::new();
    let mut stats = None;

    let mut execution = writer.commit();

    while let Some(event) = execution.next_wasm() {
        match event.kind() {
            WriteEventKind::Save => {
                let id = event.id().expect("id");
                let cached = event.recv().expect("cached");
                cache.insert(id, cached);
            }
            WriteEventKind::Load => {
                if let Some(cached) = cache.get(&event.id().expect("id")) {
                    event.send_cached(cached).expect("send");
                } else {
                    event.send_empty().expect("send empty");
                }
            }
            WriteEventKind::Stats => stats = Some(event.stats()),
        }
    }

    assert_eq!(stats.expect("some stats").expect("ok stats").documents, 2);

    let mut options = QueryOptions::default();
    options.set_maximum_distance(4);
    options.set_minimum_similarity(0.5);

    let mut query = engine
        .query()
        .with_options(options)
        .with_structured_expression(Expression::or(
            Expression::and(
                Expression::attr("creation", Func::GreaterThan, TermValue::int(10)),
                Expression::attr("creation", Func::LessThan, TermValue::int(100)),
            ),
            Expression::attr("creation", Func::Equals, TermValue::int(12356)),
        ))
        .search();

    while let Some(event) = query.next_wasm() {
        match event.kind() {
            QueryEventKind::Found => info!(found = event.found().expect("found").identifier()),
            QueryEventKind::Load => {
                if let Some(cached) = cache.get(&event.id().expect("id")) {
                    event.send_cached(cached).expect("send");
                } else {
                    event.send_empty().expect("send empty");
                }
            }
            QueryEventKind::Stats => {
                info!(stats = ?event.stats())
            }
        }
    }
}

#[wasm_bindgen_test]
fn wasm_can_work_entry_values() {
    let i = JsValue::from_f64(1.0);
    let b = JsValue::from_bool(true);
    let t = JsValue::from_str("hey");
    let tt = JsValue::from(
        [
            JsValue::try_from(TextToken::new(1, "hello".to_owned())).expect("t1"),
            JsValue::try_from(TextToken::new(2, "world".to_owned())).expect("t2"),
        ]
        .into_iter()
        .collect::<Array>(),
    );

    let e_i = EntryValue::new_wasm(i).expect("int");
    let e_b = EntryValue::new_wasm(b).expect("bool");
    let e_t = EntryValue::new_wasm(t).expect("tag");
    let e_tt = EntryValue::new_wasm(tt).expect("text");

    assert_eq!(
        Option::<proton_foundation_search::entry::EntryValue>::from(e_i),
        proton_foundation_search::entry::EntryValue::new(1)
    );
    assert_eq!(
        Option::<proton_foundation_search::entry::EntryValue>::from(e_b),
        proton_foundation_search::entry::EntryValue::new(true)
    );
    assert_eq!(
        Option::<proton_foundation_search::entry::EntryValue>::from(e_t),
        proton_foundation_search::entry::EntryValue::new("hey")
    );
    assert_eq!(
        Option::<proton_foundation_search::entry::EntryValue>::from(e_tt),
        Some(proton_foundation_search::entry::EntryValue::Text(vec![
            (1, "hello".into()),
            (2, "world".into())
        ]))
    );
}
