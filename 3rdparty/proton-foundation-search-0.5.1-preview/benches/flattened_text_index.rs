#![allow(clippy::expect_used)]
use std::collections::HashMap;
use std::io::BufReader;

use criterion::{Criterion, criterion_group, criterion_main};
use proton_foundation_search::document::Document;
use proton_foundation_search::entry::EntryValue;
use proton_foundation_search::processor::{Proc, Processor};

// Import the FlattenedTextIndex and related structs from local utility
mod flattened_util;
use flattened_util::{FlattenedTextIndex, TokenContext};
// Import the helper function for processing fixture data
use search_internal_helper::process_email_fixture_data;

use crate::flattened_util::EntryIndex;

fn insert_doc(
    index: &mut FlattenedTextIndex,
    proc: &Processor,
    entry_index: EntryIndex,
    doc: Document,
) {
    let (_size, entry) = proc.process_document(doc.clone()).expect("doc processing");

    let mut attrs = HashMap::new();

    // Process each attribute in the document using the public API
    for (field, value) in entry.attributes().iter() {
        let next = attrs.len() as u32;
        let attr = attrs.entry(field).or_insert(next);

        // Extract tokens from text attributes
        // Split text into tokens (simple whitespace splitting)
        for (idx, value) in value.iter().enumerate() {
            let EntryValue::Text(tokens) = value else {
                continue;
            };
            for (pos, token) in tokens {
                if token.is_empty() {
                    continue;
                }
                let context = TokenContext {
                    entry_index,
                    attribute_index: *attr,
                    value_index: idx as u32,
                    token_position: flattened_util::TokenPosition(*pos as u32),
                };
                index.insert_token(token, context);
            }
        }
    }
}

fn bench_flattened_index(c: &mut Criterion) {
    // Load and process fixture data once (outside measurement loop)

    let input = include_str!("fixtures/12k_emails_random.jsonl");

    // value processor
    let proc = Processor::default();

    // Process documents using the helper function
    let documents = process_email_fixture_data(
        BufReader::new(input.as_bytes()),
        "subject",
        "body",
        "from",
        "to",
        "time",
    );

    let mut group = c.benchmark_group("flattened_index");
    group.sample_size(10); // Minimum required by Criterion

    group.bench_function("build_and_serialize", |b| {
        b.iter(|| {
            // Create a new flattened index for each iteration
            let mut index = FlattenedTextIndex::default();

            // Insert documents into flattened index with proper index assignment
            for (doc_idx, (_time, doc)) in documents.iter().enumerate() {
                insert_doc(&mut index, &proc, doc_idx as EntryIndex, doc.clone());
            }

            // Serialize to disk (simulating what the writer would do)
            let _ = index.serialize_to_file("target/flattened_index.cbor");

            // Return the index to prevent optimization from removing the work
            index
        })
    });

    // Ensure the file exists for deserialize benchmark
    if !std::path::Path::new("target/flattened_index.cbor").exists() {
        // Ensure target directory exists
        std::fs::create_dir_all("target").expect("Failed to create target directory");

        // Create a sample index and serialize it
        let mut sample_index = FlattenedTextIndex::default();
        if documents.is_empty() {
            let (_, doc) = &documents[0];
            insert_doc(&mut sample_index, &proc, 0, doc.clone());
        }
        sample_index
            .serialize_to_file("target/flattened_index.cbor")
            .expect("Failed to create sample file");
    }

    group.bench_function("deserialize", |b| {
        b.iter(|| {
            // Deserialize from disk

            // Return the index to prevent optimization from removing the work
            FlattenedTextIndex::deserialize_from_file("target/flattened_index.cbor")
                .expect("Failed to deserialize index")
        })
    });

    group.bench_function("to_hierarchical", |b| {
        b.iter(|| {
            // Deserialize from disk first
            let flattened_index =
                FlattenedTextIndex::deserialize_from_file("target/flattened_index.cbor")
                    .expect("Failed to deserialize index");

            // Convert to hierarchical structure

            // Return the hierarchical index to prevent optimization from removing the work
            flattened_index.to_hierarchical()
        })
    });

    // Setup: Create and serialize segments of different sizes (done once outside timing)
    // Sizes start out with larger one representing the current state with two small ones to be merged
    let segment_configs = [
        (0, 1000),  // Segment 0: documents 0-999 (1000 docs)
        (2000, 50), // Segment 1: documents 2000-2049 (50 docs)
        (5000, 35), // Segment 2: documents 5000-5034 (35 docs)
    ];
    let num_segments = segment_configs.len();

    // Ensure target directory exists for segments
    std::fs::create_dir_all("target").expect("Failed to create target directory");

    // Only create segments if we have documents
    if !documents.is_empty() {
        // Create and serialize each segment
        for (segment_idx, (start_doc, segment_size)) in segment_configs.iter().enumerate() {
            let end_doc = std::cmp::min(start_doc + segment_size, documents.len());
            let segment_docs = &documents[*start_doc..end_doc];

            let mut segment = FlattenedTextIndex::default();
            for (doc_idx, (_time, doc)) in segment_docs.iter().enumerate() {
                insert_doc(
                    &mut segment,
                    &proc,
                    (start_doc + doc_idx) as EntryIndex,
                    doc.clone(),
                );
            }

            // Serialize segment to disk
            let segment_path = format!("target/segment_{}.cbor", segment_idx);
            segment
                .serialize_to_file(&segment_path)
                .expect("Failed to serialize segment");
        }
    }

    group.bench_function("segment_compaction", |b| {
        b.iter(|| {
            // Step 1: Deserialize segments and compact
            let mut segments = Vec::new();

            // Only try to deserialize segments if they exist
            if !documents.is_empty() {
                for segment_idx in 0..num_segments {
                    let segment_path = format!("target/segment_{}.cbor", segment_idx);
                    let segment = FlattenedTextIndex::deserialize_from_file(&segment_path)
                        .expect("Failed to deserialize segment");
                    segments.push(segment);
                }
            } else {
                // Create a dummy segment if no documents exist
                segments.push(FlattenedTextIndex::default());
            }

            // Step 2: Compact all segments into one
            let compacted_index = FlattenedTextIndex::compact(segments);

            // Step 3: Serialize the compacted result
            compacted_index
                .serialize_to_file("target/compacted_index.cbor")
                .expect("Failed to serialize compacted index");

            // Return the compacted index to prevent optimization from removing the work
            compacted_index
        })
    });

    group.finish();
}

criterion_group!(benches, bench_flattened_index);
criterion_main!(benches);
