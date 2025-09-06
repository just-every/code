extern crate htmd;

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use htmd::convert;

fn benchmark(c: &mut Criterion) {
    c.bench_function("convert()", |bencher| {
        let path = "examples/page-to-markdown/html/Elon Musk - Wikipedia.html";
        let html = std::fs::read_to_string(path).unwrap();
        bencher.iter(|| convert(&html).unwrap())
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(20));
    targets = benchmark
);
criterion_main!(benches);
