#![allow(clippy::pedantic)]

mod profiler;

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use proton_mail_html_transformer::{
    Transformer, message_detector, remote_content,
    sanitizer::{self, StripStyleSheets},
    transforms::{
        self,
        styles::{BrowserCapabilities, IncludeFullStaticCss},
    },
    utm,
};

static AMOS_HTTP: &str = include_str!("./amos_http.html");
static AMOS_LANDING: &str = include_str!("./amos_landing.html");
static _IMGS: &str = include_str!("./100_imgs.html");
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
                let _ = utm::strip(tr.document());
            })
        });

        c.bench_function("remote content", |b| {
            b.iter(|| {
                let tr = tr.clone();
                remote_content::remote_content(&tr.document(), true, true);
            })
        });

        c.bench_function("strip", |b| {
            b.iter(|| {
                let tr = tr.clone();
                let _ = sanitizer::strip_whitelist(tr.document(), StripStyleSheets::No);
            })
        });

        c.bench_function("inject style", |b| {
            b.iter(|| {
                let tr = tr.clone();
                transforms::styles::inject_dark_mode(
                    tr.document(),
                    tr.document(),
                    transforms::styles::InjectDarkModeOptions {
                        sender: Some("test@pm.me"),
                        mode: transforms::ColorMode::LightMode,
                        capabilities: BrowserCapabilities {
                            supports_dark_mode_via_media_query: true,
                        },
                        root_selector: "#protonmail-message".to_owned(),
                        include_full_static_css: IncludeFullStaticCss::No,
                        trusted_senders: &[],
                    },
                );
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

    parse_inner(c, AMOS_LANDING);
    parse_inner(c, AMOS_HTTP);
}

pub fn all_transforms(c: &mut Criterion) {
    pub fn parse_inner(c: &mut Criterion, html: &str) {
        c.bench_function("All passes", |b| {
            b.iter(|| {
                let mut t = Transformer::new(html);
                t.strip_utm();
                t.disable_content(true, true);
                t.inject_ios_content_size();
                _ = t.strip_whitelist(StripStyleSheets::No);
                t.inject_dark_mode(
                    "test@pm.me",
                    transforms::ColorMode::LightMode,
                    BrowserCapabilities {
                        supports_dark_mode_via_media_query: true,
                    },
                    IncludeFullStaticCss::No,
                    &[],
                );
                _ = t.strip_blockquote();
                let tok = t.add_noreferrer();
                t.insert_links(tok);
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
