#![allow(clippy::expect_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::{Engine, WriteEvent};
use proton_foundation_search::serialization::SerDes;

fn bench_insert_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_scalability");
    group.sample_size(10);

    // Test insert performance at different index sizes
    let index_sizes = vec![100, 1000, 5000, 10000];

    let title_field = "title";
    let body_field = "body";
    let time_field = "time";

    for base_size in index_sizes {
        group.bench_function(format!("insert_after_{}_docs", base_size), |b| {
            let engine = Engine::builder()
                .build();

            let mut writer = engine.write().expect("Failed to get writer");

            // First, populate the index with base_size documents
            for i in 0..base_size {
                let doc_id = format!("doc_{}", i);
                let title = format!("Title for document {}", i);
                let body = format!("This is the body content for document {}. It contains various words and phrases that will be indexed.", i);
                let doc = Document::new(&doc_id)
                    .with_attribute(title_field, Value::text(title))
                    .with_attribute(body_field, Value::text(body))
                    .with_attribute(time_field, i as u64);
                writer.insert(doc).expect("Failed to insert document");
            }

            // Commit to ensure all data is in the index structure
            for event in writer.commit()
            {
                let _bytes = handle_write(event);
            }

            b.iter(|| {
                    // Get a new writer for the additional insert
                    let mut writer = engine.write().expect("Failed to get writer");

                    // Now measure the time to insert one more document
                    // This should show O(log n) scaling as the index grows
                    let new_doc_id = format!("doc_{}", base_size);
                    let new_title = format!("Title for document {}", base_size);
                    let new_body = format!("This is the body content for document {}. It contains various words and phrases that will be indexed.", base_size);
                    let new_doc = Document::new(&new_doc_id)
                        .with_attribute(title_field, Value::text(new_title))
                        .with_attribute(body_field, Value::text(new_body))
                        .with_attribute(time_field, base_size as u64);
                    // This insert operation should take longer as base_size increases
                    // due to the nested BTreeMap structure maintenance
                    writer.insert(new_doc).expect("Failed to insert new document");

                    // Commit to ensure all data is in the index structure
                    let mut blobs = vec![];
                    for event in writer.commit()
                    {
                        if let Some(name)= handle_write(event)
                        {
                            blobs.push(name);
                        }
                    }
                    blobs
            });
        });
    }

    group.finish();
}

fn handle_write(event: WriteEvent) -> Option<Box<str>> {
    match event {
        WriteEvent::Modified(_) => None,
        WriteEvent::Load(load_event) => {
            (load_event.send)(&SerDes::Cbor, vec![]).expect("empty send");
            None
        }
        WriteEvent::Save(save_event) => {
            (save_event.recv)(&SerDes::Cbor).expect("recv");
            Some(save_event.name)
        }
    }
}

criterion_group!(benches, bench_insert_scalability);
criterion_main!(benches);
