#![allow(clippy::expect_used)]

use std::collections::HashMap;

use criterion::{Criterion, criterion_group, criterion_main};
use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::{Engine, QueryEvent, WriteEvent};
use proton_foundation_search::query::expression::{Expression, Func};
use proton_foundation_search::serialization::SerDes;

use crate::model::Mail;

mod model;

fn criterion_benchmark(c: &mut Criterion) {
    let input = include_str!(concat!("../../../", env!("MAIL_FILE")));

    let field_subject = "subject";
    let field_body = "body";
    let field_from = "from";
    let field_to = "to";
    let field_cc = "cc";
    let field_bcc = "bcc";
    let field_time = "time";

    let engine = Engine::builder().build();

    let documents = input
        .split("\n")
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(pos, line)| {
            serde_json::from_str::<Mail>(line).unwrap_or_else(|_| panic!("mail {pos} {line:?}"))
        })
        .map(|mail| {
            let mut doc = Document::new(&mail.id)
                .with_attribute(field_time, mail.time)
                .with_attribute(field_subject, Value::text(mail.subject))
                .with_attribute(field_body, Value::text(mail.body))
                .with_attribute(field_from, Value::text(mail.from.email))
                .with_attribute(field_from, Value::text(mail.from.name));
            for rcpt in mail.to {
                doc = doc
                    .with_attribute(field_to, Value::text(rcpt.name))
                    .with_attribute(field_to, Value::text(rcpt.email));
            }
            for rcpt in mail.cc {
                doc = doc
                    .with_attribute(field_cc, Value::text(rcpt.name))
                    .with_attribute(field_cc, Value::text(rcpt.email));
            }
            for rcpt in mail.bcc {
                doc = doc
                    .with_attribute(field_bcc, Value::text(rcpt.name))
                    .with_attribute(field_bcc, Value::text(rcpt.email));
            }
            doc
        });

    let mut storage = HashMap::new();
    let expr = Expression::any_attr(Func::Matches, Value::text("hospital"));
    let engine = &engine;

    let expr_init = expr.clone();

    let mut writer = engine.write().expect("writer");

    for document in documents {
        writer.insert(document).expect("insert");
    }
    for event in writer.commit() {
        //println!("{event:?}");
        match event {
            WriteEvent::Modified(_) => {}
            WriteEvent::Save(save_event) => {
                let blob = (save_event.recv)(&SerDes::Cbor).expect("blob recv");
                storage.insert(save_event.name, blob);
            }
            WriteEvent::Load(load_event) => {
                let blob = storage.get(&load_event.name).cloned().unwrap_or_default();
                (load_event.send)(&SerDes::Cbor, blob).expect("blob send");
            }
        }
    }

    let mut results = vec![];
    for event in engine.query().with_expression(expr_init).search() {
        //println!("{event:?}");
        match event {
            QueryEvent::Load(load_event) => {
                let blob = storage.get(&load_event.name).cloned().unwrap_or_default();
                (load_event.send)(&SerDes::Cbor, blob).expect("blob send");
            }
            QueryEvent::Found(found) => results.push(found.identifier().to_owned()),
            QueryEvent::Stats(_) => {
                // could apply stats
            }
        }
    }

    assert_ne!(results.len(), 0, "no point measuring empty results");

    c.bench_function("search/engine", move |b| {
        let expr = expr.clone();
        let storage = &mut storage;
        b.iter(move || {
            let mut results = vec![];
            for event in engine.query().with_expression(expr.clone()).search() {
                match event {
                    proton_foundation_search::engine::QueryEvent::Load(load_event) => {
                        let blob = storage.get(&load_event.name).cloned().unwrap_or_default();
                        (load_event.send)(&SerDes::Cbor, blob).expect("blob send");
                    }
                    proton_foundation_search::engine::QueryEvent::Found(found) => {
                        results.push(found.identifier().to_owned())
                    }
                    QueryEvent::Stats(_) => {
                        // could apply stats
                    }
                }
            }
            results
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
