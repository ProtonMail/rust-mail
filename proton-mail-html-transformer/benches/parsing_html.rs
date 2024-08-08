#![allow(clippy::pedantic)]

mod profiler;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use proton_mail_html_transformer::{
    message_detector, remote_content, sanitizer, transforms, utm, Transformer,
};

static AMOS_HTTP: &str = include_str!("./amos_http.html");
static AMOS_LANDING: &str = include_str!("./amos_landing.html");
static IMGS: &str = include_str!("./100_imgs.html");
static LINKS: &str = include_str!("./100_links.html");

// This is for new features we're currently benchmarking so that we don't have to run every bench
pub fn current_benchmark(c: &mut Criterion) {
    pub fn parse_inner(c: &mut Criterion, html: &str) {
        let tr = Transformer::new(black_box(html));
        c.bench_function("current benchmark", |b| {
            b.iter(|| {
                let tr = tr.clone();
                message_detector::locate_blockquote(tr.document())
            })
        });
    }

    // parse_inner(c, LINKS);
    parse_inner(c, AMOS_HTTP);
    parse_inner(c, AMOS_LANDING);
}

pub fn parse(c: &mut Criterion) {
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
                utm::strip(tr.document());
            })
        });

        c.bench_function("disable remote content", |b| {
            b.iter(|| {
                let tr = tr.clone();
                remote_content::disable_remote_content(&tr.document());
            })
        });

        c.bench_function("enable remote content", |b| {
            b.iter(|| {
                let tr = tr.clone();
                remote_content::undo_disable_remote_content(&tr.document());
            })
        });

        c.bench_function("strip", |b| {
            b.iter(|| {
                let tr = tr.clone();
                sanitizer::strip_whitelist(tr.document());
            })
        });

        c.bench_function("inject style", |b| {
            b.iter(|| {
                let tr = tr.clone();
                transforms::inject_style(tr.document());
            })
        });

        c.bench_function("add noreferrer", |b| {
            b.iter(|| {
                let tr = tr.clone();
                transforms::add_noreferrer(tr.document())
            })
        });

        c.bench_function("insert_links", |b| {
            b.iter(|| {
                let tr = tr.clone();
                transforms::insert_links(tr.document())
            })
        });

        c.bench_function("proxy images", |b| {
            b.iter(|| {
                let tr = tr.clone();
                transforms::proxy_images(tr.document(), "THISISATOKEN")
            })
        });

        c.bench_function("locate blockquote", |b| {
            b.iter(|| {
                let tr = tr.clone();
                transforms::insert_links(tr.document())
            })
        });
    }

    // Benchmarks with ad-hoc inputs
    let tr = Transformer::new(black_box(LINKS));
    c.bench_function("insert 100 links", |b| {
        b.iter(|| {
            let tr = tr.clone();
            message_detector::locate_blockquote(tr.document())
        })
    });

    let tr = Transformer::new(black_box(IMGS));
    c.bench_function("proxy 100 images", |b| {
        b.iter(|| {
            let tr = tr.clone();
            transforms::proxy_images(tr.document(), "THISISATOKEN")
        })
    });

    parse_inner(c, AMOS_LANDING);
    parse_inner(c, AMOS_HTTP);
}

pub fn all_transforms(c: &mut Criterion) {
    pub fn parse_inner(c: &mut Criterion, html: &str) {
        c.bench_function("All passes", |b| {
            b.iter(|| {
                Transformer::new(html)
                    .strip_utm()
                    .enable_remote_content()
                    .disable_remote_content()
                    .inject_ios_content_size()
                    .strip_whitelist()
                    .inject_style()
                    .add_noreferrer()
                    .insert_links()
                    .proxy_images("THISISATOKEN")
                    .strip_blockquote()
                    .to_string()
            })
        });
    }

    parse_inner(c, AMOS_LANDING);
    parse_inner(c, AMOS_HTTP);
}

fn profiled() -> Criterion {
    Criterion::default().with_profiler(profiler::FlamegraphProfiler::new(100))
}

criterion_group!(
    name = benches;
    config = profiled();
    targets = current_benchmark
    // targets =  parse, all_transforms
);
criterion_main!(benches);
