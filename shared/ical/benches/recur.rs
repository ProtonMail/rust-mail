use criterion::{Criterion, criterion_group, criterion_main};
use proton_ical::RecurIterator;
use proton_ical::utils::{dt, recur};
use std::hint::black_box;

fn target(recur_s: &str, start_s: &str) -> RecurIterator {
    let recur = recur(recur_s);
    let start = dt(start_s);

    RecurIterator::new(&recur, start).unwrap()
}

fn bench(c: &mut Criterion) {
    let recur = "FREQ=MONTHLY;INTERVAL=2;BYDAY=1SU,4SU;BYHOUR=15,17;BYMINUTE=30,32;BYSECOND=11,12;COUNT=128";
    let start = "20010101T123456";

    c.bench_function("cold-next", |b| {
        b.iter(|| target(recur, start).next());
    });

    let target = target(recur, start);

    c.bench_function("warm-next", |b| {
        b.iter(|| black_box(&target).clone().next());
    });

    c.bench_function("warm-count", |b| {
        b.iter(|| black_box(&target).clone().count());
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
