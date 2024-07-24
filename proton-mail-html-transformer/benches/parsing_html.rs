#![allow(clippy::pedantic)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};

use proton_mail_html_transformer::Transformer;

pub fn parse(c: &mut Criterion) {
    let html = include_str!("./amos_landing.html");
    parse_inner(c, html);
    let html = include_str!("./amos_http.html");
    parse_inner(c, html);
}

pub fn parse_inner(c: &mut Criterion, html: &str) {
    c.bench_function("parse html", |b| {
        b.iter(|| Transformer::new(black_box(html)).to_string())
    });

    c.bench_function("disable remote content", |b| {
        b.iter(|| {
            let mut t = Transformer::new(black_box(html));
            t.disable_remote_content().unwrap();
            t.to_string();
        })
    });

    c.bench_function("strip", |b| {
        b.iter(|| {
            let t = Transformer::new(black_box(html));
            t.strip_whitelist();
            t.to_string();
        })
    });
}

criterion_group!(benches, parse);
criterion_main!(benches);
