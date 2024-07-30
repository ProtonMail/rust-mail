#![allow(clippy::pedantic)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};

use proton_mail_html_transformer::{remote_content, sanitizer, transforms, utm, Transformer};

pub fn parse(c: &mut Criterion) {
    let html = include_str!("./amos_landing.html");
    parse_inner(c, html);
    let html = include_str!("./amos_http.html");
    parse_inner(c, html);
}

pub fn parse_inner(c: &mut Criterion, html: &str) {
    c.bench_function("parse html", |b| {
        b.iter(|| Transformer::new(black_box(html)))
    });

    let tr = Transformer::new(black_box(html));

    c.bench_function("serialize html", |b| {
        b.iter(|| {
            let tr = tr.clone();
            tr.to_string();
        })
    });

    c.bench_function("strip utm", |b| {
        b.iter(|| {
            let tr = tr.clone();
            utm::strip(tr.document().clone()).unwrap();
        })
    });

    c.bench_function("disable remote content", |b| {
        b.iter(|| {
            let tr = tr.clone();
            remote_content::disable_remote_content(&tr.document().clone());
        })
    });

    c.bench_function("enable remote content", |b| {
        b.iter(|| {
            let tr = tr.clone();
            remote_content::undo_disable_remote_content(&tr.document().clone());
        })
    });

    c.bench_function("strip", |b| {
        b.iter(|| {
            let tr = tr.clone();
            sanitizer::strip_whitelist(tr.document().clone());
        })
    });

    c.bench_function("inject style", |b| {
        b.iter(|| {
            let tr = tr.clone();
            transforms::inject_style(tr.document().clone());
        })
    });

    c.bench_function("add noreferrer", |b| {
        b.iter(|| {
            let tr = tr.clone();
            transforms::add_noreferrer(tr.document().clone())
        })
    });

    c.bench_function("insert_links", |b| {
        b.iter(|| {
            let tr = tr.clone();
            transforms::insert_links(tr.document().clone())
        })
    });

    c.bench_function("All passes", |b| {
        b.iter(|| {
            let mut tr = tr.clone();
            tr.strip_utm()
                .enable_remote_content()
                .disable_remote_content()
                .inject_ios_content_size()
                .strip_whitelist()
                .inject_style()
                .add_noreferrer()
                .insert_links()
                .to_string();
        })
    });
}

criterion_group!(benches, parse);
criterion_main!(benches);
