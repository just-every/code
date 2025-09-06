#[macro_use]
extern crate criterion;

extern crate path_clean;

use criterion::black_box;
use criterion::Criterion;

use path_clean::clean;

fn clean_benchmark(c: &mut Criterion) {
    c.bench_function("clean", |b| {
        b.iter(|| clean(black_box("abc/../../././../def")))
    });
}

criterion_group!(benches, clean_benchmark);
criterion_main!(benches);
