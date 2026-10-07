//! Cascade-retraction latency on generated provenance graphs.
//!
//! Uses the same generator as `tests/retract_reference.rs` (real triples,
//! generated triples citing 1–3 earlier triples, ~5 % back-citations). For each
//! size, the root is the entity whose retraction set is largest among a fixed
//! sample of 50 entities, and the set size is printed alongside the timing.
//! Sizes: 1k, 10k, 100k and 1M generated triples (the last is ~5M rows).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use loka_core::retract_set;

#[allow(dead_code)]
#[path = "../tests/retract_reference.rs"]
mod graphgen;

fn bench_retract(c: &mut Criterion) {
    let mut group = c.benchmark_group("retract_set");
    for n in [1_000usize, 10_000, 100_000, 1_000_000] {
        // The 1M graph (~5M rows) makes each call take a large fraction of a
        // second; criterion's minimum sample count keeps the run bounded.
        group.sample_size(if n >= 1_000_000 { 10 } else { 100 });
        let g = graphgen::generate(7, n / 4, n, n);
        let root = g
            .entities
            .iter()
            .take(50)
            .copied()
            .max_by_key(|&e| retract_set(e, &g.store, &g.dict).total())
            .expect("entities");
        let set = retract_set(root, &g.store, &g.dict);
        eprintln!(
            "retract_set n={n}: store rows {}, removed {} triples, max depth {}",
            g.store.len(),
            set.total(),
            set.max_depth()
        );
        group.bench_with_input(BenchmarkId::new("generated_triples", n), &n, |b, _| {
            b.iter(|| retract_set(black_box(root), &g.store, &g.dict))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_retract);
criterion_main!(benches);
