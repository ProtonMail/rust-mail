//! Test suite for the Web and headless browsers.

#![cfg(all(target_family = "wasm", feature = "wasm-bindgen"))]

use std::collections::HashMap;

use proton_foundation_search::document::Document;
use proton_foundation_search::document::wasm::Value;
use proton_foundation_search::engine::Engine;
use proton_foundation_search::engine::wasm::{QueryEventKind, WriteEventKind};
use proton_foundation_search::query::expression::Func;
use proton_foundation_search::query::expression::wasm::Expression;
use proton_foundation_search::query::option::QueryOptions;
use proton_foundation_search::serialization::SerDes;
use tracing::info;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
async fn should_load_stats() {
    proton_foundation_search::setup::enable_tracing();
    let engine = Engine::builder().build();
    let stats = engine.stats();
    assert_eq!(stats.documents_total, None);
}

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
    writer.insert(doc).unwrap();

    let mut doc = Document::new("bar.txt");
    doc.add_attribute_value(title.into(), Value::text("Hello Another World"));
    doc.add_attribute_value(creation.into(), Value::int(12356));
    writer.insert(doc).unwrap();

    let mut storage = HashMap::new();

    let mut execution = writer.commit();

    while let Some(event) = execution.next_wasm() {
        match event.kind() {
            WriteEventKind::Load => {
                let blob = storage.get(&event.name()).cloned().unwrap_or_default();
                event.send(SerDes::Cbor, blob).expect("send");
            }
            WriteEventKind::Save => {
                let name = event.name();
                let blob = event.recv(SerDes::Cbor).expect("recv");
                storage.insert(name, blob);
            }
            WriteEventKind::Modified => {
                // ignored
            }
        }
    }

    let mut options = QueryOptions::default();
    options.set_maximum_distance(4);
    options.set_minimum_similarity(0.5);

    let mut query = engine
        .query()
        .with_options(options)
        .with_structured_expression(Expression::or(
            Expression::and(
                Expression::attr("creation", Func::GreaterThan, Value::int(10)),
                Expression::attr("creation", Func::LessThan, Value::int(100)),
            ),
            Expression::attr("creation", Func::Equals, Value::int(12356)),
        ))
        .search();

    while let Some(event) = query.next_wasm() {
        match event.kind() {
            QueryEventKind::Found => info!(found = event.name()),
            QueryEventKind::Load => {
                let blob = storage.get(&event.name()).cloned().unwrap_or_default();
                event.send(SerDes::Cbor, blob).expect("send");
            }
            QueryEventKind::Stats => {
                info!(stats = ?event.stats())
            }
        }
    }
}
