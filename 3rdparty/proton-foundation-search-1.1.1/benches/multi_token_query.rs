//! Benchmarks for multi-token query performance optimization.
//!
//! This benchmark measures the performance of AND queries with multiple tokens,
//! specifically testing the sequential execution with progressive filtering optimization.
#![allow(clippy::expect_used)]

use std::collections::HashMap;

use criterion::{Criterion, criterion_group, criterion_main};
use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::{Engine, QueryEvent, WriteEvent};
use proton_foundation_search::query::expression::Expression;

/// Configuration for benchmark scenarios
struct BenchConfig {
    num_entries: usize,
    /// Probability that an entry contains token_alpha
    token_a_selectivity: f64,
    /// Probability that an entry contains token_beta
    token_b_selectivity: f64,
    /// Probability that an entry contains token_gamma
    token_c_selectivity: f64,
    /// Number of common tokens per entry (for realistic content)
    common_tokens_per_entry: usize,
}

/// Generate content with probabilistic token presence
fn generate_content(config: &BenchConfig, entry_id: usize) -> String {
    let mut tokens = Vec::new();

    // Add common tokens for realistic content
    for i in 0..config.common_tokens_per_entry {
        tokens.push(format!("common{}", (entry_id + i) % 100));
    }

    // Use deterministic pseudo-random based on entry_id
    let hash = |seed: usize, id: usize| -> f64 {
        let x = ((id * 1103515245 + seed) % (1 << 16)) as f64;
        x / (1 << 16) as f64
    };

    // Probabilistically add benchmark tokens based on selectivity
    if hash(12345, entry_id) < config.token_a_selectivity {
        tokens.push("searchtokenalpha".to_string());
    }
    if hash(67890, entry_id) < config.token_b_selectivity {
        tokens.push("searchtokenbeta".to_string());
    }
    if hash(11111, entry_id) < config.token_c_selectivity {
        tokens.push("searchtokengamma".to_string());
    }

    tokens.join(" ")
}

/// Build an engine with the given configuration
fn build_engine(
    config: &BenchConfig,
) -> (
    Engine,
    HashMap<proton_foundation_search::blob::BlobId, proton_foundation_search::blob::Cached>,
) {
    let engine = Engine::builder().build();
    let mut cache = HashMap::new();

    let mut writer = engine.write().expect("writer");

    for i in 0..config.num_entries {
        let id = format!("entry_{}", i);
        let content = generate_content(config, i);
        let doc = Document::new(&id).with_attribute("content", Value::text(content));
        writer.insert(doc);
    }

    for event in writer.commit() {
        match event {
            WriteEvent::Save(save_event) => {
                cache.insert(save_event.id(), save_event.recv());
            }
            WriteEvent::Load(load_event) => {
                if let Some(cached) = cache.get(&load_event.id()) {
                    load_event.send_cached(cached).expect("send cached");
                } else {
                    load_event.send_empty().expect("send empty");
                }
            }
            WriteEvent::Stats(_) => {}
        }
    }

    (engine, cache)
}

/// Execute a query and count results
fn execute_query(
    engine: &Engine,
    cache: &HashMap<proton_foundation_search::blob::BlobId, proton_foundation_search::blob::Cached>,
    query_str: &str,
) -> usize {
    let expr: Expression = query_str.parse().expect("Failed to parse query");
    let search = engine.query().with_expression(expr).search();

    let mut count = 0;
    for event in search {
        match event {
            QueryEvent::Load(load_event) => {
                if let Some(cached) = cache.get(&load_event.id()) {
                    load_event.send_cached(cached).expect("send cached");
                } else {
                    load_event.send_empty().expect("send empty");
                }
            }
            QueryEvent::Found(_) => {
                count += 1;
            }
            QueryEvent::Stats(_) => {}
        }
    }
    count
}

fn bench_high_selectivity(c: &mut Criterion) {
    let config = BenchConfig {
        num_entries: 10_000,
        token_a_selectivity: 0.5, // 50% of entries
        token_b_selectivity: 0.3, // 30% of entries
        token_c_selectivity: 0.2, // 20% of entries
        common_tokens_per_entry: 10,
    };

    println!("\n=== Building HIGH SELECTIVITY (varied overlap) ===");
    println!(
        "Entries: {}, token_a: {}%, token_b: {}%, token_c: {}%",
        config.num_entries,
        (config.token_a_selectivity * 100.0) as usize,
        (config.token_b_selectivity * 100.0) as usize,
        (config.token_c_selectivity * 100.0) as usize,
    );
    println!(
        "Expected: A={}, B={}, A∩B≈{}, A∩B∩C≈{}",
        (config.num_entries as f64 * config.token_a_selectivity) as usize,
        (config.num_entries as f64 * config.token_b_selectivity) as usize,
        (config.num_entries as f64 * config.token_a_selectivity * config.token_b_selectivity)
            as usize,
        (config.num_entries as f64
            * config.token_a_selectivity
            * config.token_b_selectivity
            * config.token_c_selectivity) as usize,
    );

    let start = std::time::Instant::now();
    let (engine, cache) = build_engine(&config);
    println!("Built in {:?}", start.elapsed());

    // Print actual counts
    println!("\n=== ACTUAL COUNTS ===");
    let count_a = execute_query(&engine, &cache, "searchtokenalpha");
    let count_ab = execute_query(&engine, &cache, "searchtokenalpha searchtokenbeta");
    let count_abc = execute_query(
        &engine,
        &cache,
        "searchtokenalpha searchtokenbeta searchtokengamma",
    );
    println!("token_alpha alone:        {} matches", count_a);
    println!(
        "alpha AND beta:           {} matches (filtered {}%)",
        count_ab,
        100 - (count_ab * 100 / count_a.max(1))
    );
    println!(
        "alpha AND beta AND gamma: {} matches (filtered {}%)",
        count_abc,
        100 - (count_abc * 100 / count_a.max(1))
    );

    let mut group = c.benchmark_group("high_selectivity");

    // Single token baseline
    group.bench_function("1_single_alpha", |b| {
        b.iter(|| execute_query(&engine, &cache, "searchtokenalpha"))
    });

    group.bench_function("2_single_beta", |b| {
        b.iter(|| execute_query(&engine, &cache, "searchtokenbeta"))
    });

    // Two-token AND query
    group.bench_function("3_alpha_AND_beta", |b| {
        b.iter(|| execute_query(&engine, &cache, "searchtokenalpha searchtokenbeta"))
    });

    // Three-token AND query
    group.bench_function("4_alpha_AND_beta_AND_gamma", |b| {
        b.iter(|| {
            execute_query(
                &engine,
                &cache,
                "searchtokenalpha searchtokenbeta searchtokengamma",
            )
        })
    });

    group.finish();
}

fn bench_low_selectivity(c: &mut Criterion) {
    let config = BenchConfig {
        num_entries: 10_000,
        token_a_selectivity: 0.9, // 90% of entries
        token_b_selectivity: 0.9, // 90% of entries
        token_c_selectivity: 0.9, // 90% of entries
        common_tokens_per_entry: 10,
    };

    println!("\n=== Building LOW SELECTIVITY (high overlap - baseline) ===");
    println!(
        "Entries: {}, token_a: {}%, token_b: {}%, token_c: {}%",
        config.num_entries,
        (config.token_a_selectivity * 100.0) as usize,
        (config.token_b_selectivity * 100.0) as usize,
        (config.token_c_selectivity * 100.0) as usize,
    );
    println!(
        "Expected: A={}, B={}, A∩B≈{}, A∩B∩C≈{}",
        (config.num_entries as f64 * config.token_a_selectivity) as usize,
        (config.num_entries as f64 * config.token_b_selectivity) as usize,
        (config.num_entries as f64 * config.token_a_selectivity * config.token_b_selectivity)
            as usize,
        (config.num_entries as f64
            * config.token_a_selectivity
            * config.token_b_selectivity
            * config.token_c_selectivity) as usize,
    );

    let start = std::time::Instant::now();
    let (engine, cache) = build_engine(&config);
    println!("Built in {:?}", start.elapsed());

    // Print actual counts
    println!("\n=== ACTUAL COUNTS ===");
    let count_a = execute_query(&engine, &cache, "searchtokenalpha");
    let count_ab = execute_query(&engine, &cache, "searchtokenalpha searchtokenbeta");
    let count_abc = execute_query(
        &engine,
        &cache,
        "searchtokenalpha searchtokenbeta searchtokengamma",
    );
    println!("token_alpha alone:        {} matches", count_a);
    println!(
        "alpha AND beta:           {} matches (filtered {}%)",
        count_ab,
        100 - (count_ab * 100 / count_a.max(1))
    );
    println!(
        "alpha AND beta AND gamma: {} matches (filtered {}%)",
        count_abc,
        100 - (count_abc * 100 / count_a.max(1))
    );

    let mut group = c.benchmark_group("low_selectivity");

    // Single token baseline
    group.bench_function("1_single_alpha", |b| {
        b.iter(|| execute_query(&engine, &cache, "searchtokenalpha"))
    });

    // Two-token AND query
    group.bench_function("2_alpha_AND_beta", |b| {
        b.iter(|| execute_query(&engine, &cache, "searchtokenalpha searchtokenbeta"))
    });

    // Three-token AND query
    group.bench_function("3_alpha_AND_beta_AND_gamma", |b| {
        b.iter(|| {
            execute_query(
                &engine,
                &cache,
                "searchtokenalpha searchtokenbeta searchtokengamma",
            )
        })
    });

    group.finish();
}

criterion_group!(benches, bench_high_selectivity, bench_low_selectivity);
criterion_main!(benches);
