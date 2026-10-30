//! External baseline for cascade retraction (paper §6.2): the same retraction
//! set computed by plain SPARQL-star queries, timed against `retract_set` on
//! the same generated graphs, in the same process.
//!
//! Graphs, sizes and root choice are those of `loka-core/benches/retract.rs`.
//! Before timing, the SPARQL result is checked to be exactly `retract_set`'s
//! set. The SPARQL side is three queries:
//!
//! 1. the rows with the root as subject or object;
//! 2. the reserved-namespace annotations of those rows;
//! 3. the closure: every quoted triple `?g` with `?g sel* ?h` where `?h`
//!    cites a triple touching the root, with all of `?g`'s reserved
//!    annotations. `?g`'s own asserted row is its dereferenced quoted triple.
//!
//! The one-hop-then-`sel*` form is needed because the engine evaluates a
//! property path only between variables or IRIs, not into a quoted pattern.

use std::collections::HashSet;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use loka_core::{quoted_triple_id, retract_set, TermDictionary, TermId, Triple, TripleStore};
use loka_sparql::{execute, parser, Query};

#[allow(dead_code)]
#[path = "../../loka-core/tests/retract_reference.rs"]
mod graphgen;

const SEL: &str = "<http://loka.dev/provenance/propositionSelectedFrom>";
const RESERVED: &str = "FILTER(STRSTARTS(STR(?ap), \"http://loka.dev/provenance/\"))";

struct Queries {
    touching: Query,
    touching_ann: Query,
    closure: Query,
}

fn queries(e: &str) -> Queries {
    let p = |q: String| parser::parse(&q).expect("baseline query parses");
    Queries {
        touching: p(format!(
            "SELECT ?s ?p ?o WHERE {{ {{ <{e}> ?p ?o }} UNION {{ ?s ?p <{e}> }} }}"
        )),
        touching_ann: p(format!(
            "SELECT ?s ?p ?o ?ap ?av WHERE {{ {{ << <{e}> ?p ?o >> ?ap ?av }} \
             UNION {{ << ?s ?p <{e}> >> ?ap ?av }} {RESERVED} }}"
        )),
        closure: p(format!(
            "SELECT DISTINCT ?g ?ap ?av WHERE {{ {{ ?h {SEL} << <{e}> ?tp ?to >> }} \
             UNION {{ ?h {SEL} << ?ts ?tp <{e}> >> }} ?g {SEL}* ?h . ?g ?ap ?av {RESERVED} }}"
        )),
    }
}

/// The retraction set of `root` from the three SPARQL queries.
fn sparql_set(
    root: TermId,
    q: &Queries,
    store: &TripleStore,
    dict: &TermDictionary,
) -> HashSet<Triple> {
    let mut out = HashSet::new();
    let get = |row: &loka_sparql::Bindings, v: &str| row.get(v).copied().unwrap_or(root);
    for row in execute(&q.touching, store, dict).expect("query 1").rows {
        out.insert(Triple::new(get(&row, "s"), get(&row, "p"), get(&row, "o")));
    }
    for row in execute(&q.touching_ann, store, dict).expect("query 2").rows {
        let qid = quoted_triple_id(get(&row, "s"), get(&row, "p"), get(&row, "o"));
        out.insert(Triple::new(qid, get(&row, "ap"), get(&row, "av")));
    }
    for row in execute(&q.closure, store, dict).expect("query 3").rows {
        let g = get(&row, "g");
        out.insert(Triple::new(g, get(&row, "ap"), get(&row, "av")));
        if let Some((s, p, o)) = dict.resolve_quoted(g) {
            out.insert(Triple::new(s, p, o));
        }
    }
    out
}

fn bench_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("retract_baseline");
    for n in [1_000usize, 10_000, 100_000, 1_000_000] {
        group.sample_size(10);
        let g = graphgen::generate(7, n / 4, n, n);
        let root = g
            .entities
            .iter()
            .take(50)
            .copied()
            .max_by_key(|&e| retract_set(e, &g.store, &g.dict).total())
            .expect("entities");
        let e = g.dict.render_term(root).expect("root IRI");
        let e = e.trim_start_matches('<').trim_end_matches('>').to_string();
        let q = queries(&e);

        let engine: HashSet<Triple> = retract_set(root, &g.store, &g.dict)
            .by_depth
            .into_iter()
            .flatten()
            .collect();
        let sparql = sparql_set(root, &q, &g.store, &g.dict);
        eprintln!(
            "retract_baseline n={n}: store rows {}, retract_set {} triples, SPARQL {} triples, equal {}",
            g.store.len(),
            engine.len(),
            sparql.len(),
            engine == sparql
        );
        assert_eq!(engine, sparql, "SPARQL closure differs from retract_set at n={n}");

        group.bench_with_input(BenchmarkId::new("retract_set", n), &n, |b, _| {
            b.iter(|| retract_set(black_box(root), &g.store, &g.dict))
        });
        group.bench_with_input(BenchmarkId::new("sparql_closure", n), &n, |b, _| {
            b.iter(|| sparql_set(black_box(root), &q, &g.store, &g.dict))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_baseline);
criterion_main!(benches);
