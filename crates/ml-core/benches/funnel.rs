use criterion::{black_box, criterion_group, criterion_main, Criterion};
use neural_router_data::load_fixture;
use neural_router_domain::Policy;
use neural_router_ml::funnel;

fn bench_funnel(c: &mut Criterion) {
    let chain = load_fixture().expect("fixture");
    let policy = Policy::file_default();
    c.bench_function("funnel_fixture", |b| {
        b.iter(|| funnel(black_box(&chain), black_box(&policy)))
    });
}

criterion_group!(benches, bench_funnel);
criterion_main!(benches);
