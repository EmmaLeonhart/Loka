//! Adaptive execution v1: commuting patterns reordered mid-query from sampled
//! row counts (`planning/adaptive-execution.md`).

use loka_core::{DatabaseConfig, TermDictionary, Triple, TripleStore};
use loka_hnsw::VectorRegistry;
use loka_sparql::{execute_instrumented, parse, QueryMetrics};

const EX: &str = "http://example.org/";

/// The `adaptive_gap` data at test size: 2000 `?x :p ?y` over 20 hubs, 50
/// `:q` per hub (the bench uses 500), `:s` on x0 and x1 only (and on 10,000
/// unrelated subjects). Still clears every gate: 2000 rows in hand, and
/// 100k estimated rows for `q` against ~60 for `s`.
fn correlated() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let id = |d: &mut TermDictionary, s: &str| d.intern(&format!("{EX}{s}"));
    let (p, q, s) = (id(&mut dict, "p"), id(&mut dict, "q"), id(&mut dict, "s"));
    for i in 0..2000 {
        let x = id(&mut dict, &format!("x{i}"));
        let y = id(&mut dict, &format!("hub{}", i % 20));
        store.insert(Triple::new(x, p, y)).unwrap();
        if i < 2 {
            let w = id(&mut dict, &format!("w{i}"));
            store.insert(Triple::new(x, s, w)).unwrap();
        }
    }
    for h in 0..20 {
        let y = id(&mut dict, &format!("hub{h}"));
        for j in 0..50 {
            let z = id(&mut dict, &format!("z{h}_{j}"));
            store.insert(Triple::new(y, q, z)).unwrap();
        }
    }
    for k in 0..10_000 {
        let o = id(&mut dict, &format!("other{k}"));
        let w = id(&mut dict, &format!("ow{k}"));
        store.insert(Triple::new(o, s, w)).unwrap();
    }
    (store, dict)
}

/// Sorted rows (a multiset) and the number of adaptive reorders.
fn run(
    query: &str,
    store: &TripleStore,
    dict: &TermDictionary,
    adaptive: bool,
) -> (Vec<String>, u64) {
    let q = parse(&format!("PREFIX ex: <{EX}> {query}")).unwrap();
    let metrics = QueryMetrics::new();
    let config = DatabaseConfig {
        adaptive_execution: adaptive,
        ..Default::default()
    };
    let (result, _) = execute_instrumented(
        &q,
        store,
        dict,
        &VectorRegistry::new(),
        &config,
        None,
        Some(&metrics),
    )
    .unwrap();
    let mut rows: Vec<String> = result
        .rows
        .iter()
        .map(|r| {
            let mut cells: Vec<String> = r
                .iter()
                .filter(|(k, _)| result.columns.contains(k))
                .map(|(k, v)| format!("{k}={}", dict.resolve(*v).unwrap_or("?")))
                .collect();
            cells.sort();
            cells.join(" ")
        })
        .collect();
    rows.sort();
    (rows, metrics.report().adaptive_reorders)
}

const GAP: &str = "SELECT ?x ?z WHERE { ?x ex:p ?y . ?y ex:q ?z . ?x ex:s ?w }";

#[test]
fn the_gap_query_is_reordered_with_the_same_rows() {
    let (store, dict) = correlated();
    let (fixed, n0) = run(GAP, &store, &dict, false);
    let (adaptive, n1) = run(GAP, &store, &dict, true);
    assert_eq!(n0, 0, "off means off");
    assert_eq!(
        n1, 1,
        "after ?x ex:p ?y, the selective ?x ex:s ?w moves ahead of ?y ex:q ?z"
    );
    assert_eq!(fixed.len(), 100);
    assert_eq!(adaptive, fixed);
}

#[test]
fn already_good_order_is_left_alone() {
    let (store, dict) = correlated();
    let best = "SELECT ?x ?z WHERE { ?x ex:p ?y . ?x ex:s ?w . ?y ex:q ?z }";
    let (_, n) = run(best, &store, &dict, true);
    assert_eq!(n, 0);
}

#[test]
fn a_filter_is_crossed_only_by_patterns_it_does_not_read() {
    let (store, dict) = correlated();
    // The filter reads only ?z; `?x ex:s ?w` binds ?x, ?w: it may move ahead.
    let disjoint =
        "SELECT ?x ?z WHERE { ?x ex:p ?y . ?y ex:q ?z . FILTER(?z != ex:z0_1) ?x ex:s ?w }";
    let (fixed, _) = run(disjoint, &store, &dict, false);
    let (adaptive, n) = run(disjoint, &store, &dict, true);
    assert_eq!(n, 1, "moved past a filter it doesn't affect");
    assert_eq!(adaptive, fixed);
    assert!(!fixed.is_empty());

    // The filter reads ?w, which `?x ex:s ?w` would bind: moving it ahead
    // could change the filter's value, so it must not move. Same rows
    // either way: here, rows survive only because ?w is still unbound.
    for q in [
        "SELECT ?x ?z WHERE { ?x ex:p ?y . ?y ex:q ?z . FILTER(!BOUND(?w)) ?x ex:s ?w }",
        "SELECT ?x ?z WHERE { ?x ex:p ?y . ?y ex:q ?z . FILTER(?w != ex:w0) ?x ex:s ?w }",
    ] {
        let (fixed, _) = run(q, &store, &dict, false);
        let (adaptive, n) = run(q, &store, &dict, true);
        assert_eq!(n, 0, "{q}");
        assert_eq!(adaptive, fixed, "{q}");
    }
    let (rows, _) = run(
        "SELECT ?x ?z WHERE { ?x ex:p ?y . ?y ex:q ?z . FILTER(!BOUND(?w)) ?x ex:s ?w }",
        &store,
        &dict,
        true,
    );
    assert!(!rows.is_empty(), "jumping ?w ahead would have emptied this");
}

#[test]
fn other_barriers_are_never_crossed() {
    let (store, dict) = correlated();
    for q in [
        // EXISTS filters stay barriers: their inner patterns can mention anything.
        "SELECT ?x ?z WHERE { ?x ex:p ?y . ?y ex:q ?z . FILTER EXISTS { ?x ex:p ?y } ?x ex:s ?w }",
        "SELECT ?x ?z ?w WHERE { ?x ex:p ?y . OPTIONAL { ?x ex:s ?w } ?y ex:q ?z }",
        "SELECT ?x ?z WHERE { ?x ex:p ?y . VALUES ?y { ex:hub3 } ?y ex:q ?z . ?x ex:s ?w }",
    ] {
        let (fixed, _) = run(q, &store, &dict, false);
        let (adaptive, _) = run(q, &store, &dict, true);
        assert_eq!(adaptive, fixed, "{q}");
    }
    for q in [
        "SELECT ?x ?z WHERE { ?x ex:p ?y . ?y ex:q ?z . FILTER EXISTS { ?x ex:p ?y } ?x ex:s ?w }",
        "SELECT ?x ?z ?w WHERE { ?x ex:p ?y . OPTIONAL { ?x ex:s ?w } ?y ex:q ?z }",
    ] {
        let (_, n) = run(q, &store, &dict, true);
        assert_eq!(n, 0, "{q}");
    }
}

#[test]
fn other_queries_give_the_same_rows_either_way() {
    let (store, dict) = correlated();
    for q in [
        "SELECT ?x ?y WHERE { ?x ex:p ?y . ?x ex:s ?w }",
        "SELECT ?y ?z WHERE { ?y ex:q ?z . ?x ex:p ?y . ?x ex:s ?w }",
        "SELECT ?x WHERE { ?x ex:p ex:hub3 . ?x ex:p ?y . ?y ex:q ?z }",
        // Shares ?w with the first pattern; the fixed order stays small.
        "SELECT ?x ?w WHERE { ?x ex:s ?w . ?x ex:p ?y . ?y ex:q ex:z3_7 }",
    ] {
        let (fixed, _) = run(q, &store, &dict, false);
        let (adaptive, _) = run(q, &store, &dict, true);
        assert_eq!(adaptive, fixed, "{q}");
    }
}

#[test]
fn small_intermediate_results_are_not_reordered() {
    // Same shape at 1/10 the size: never 1,000 rows in hand, so no sampling.
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let id = |d: &mut TermDictionary, s: &str| d.intern(&format!("{EX}{s}"));
    let (p, q, s) = (id(&mut dict, "p"), id(&mut dict, "q"), id(&mut dict, "s"));
    for i in 0..200 {
        let x = id(&mut dict, &format!("x{i}"));
        let y = id(&mut dict, &format!("hub{}", i % 20));
        store.insert(Triple::new(x, p, y)).unwrap();
        if i < 2 {
            let w = id(&mut dict, &format!("w{i}"));
            store.insert(Triple::new(x, s, w)).unwrap();
        }
    }
    for h in 0..20 {
        let y = id(&mut dict, &format!("hub{h}"));
        for j in 0..50 {
            let z = id(&mut dict, &format!("z{h}_{j}"));
            store.insert(Triple::new(y, q, z)).unwrap();
        }
    }
    let (fixed, _) = run(GAP, &store, &dict, false);
    let (adaptive, n) = run(GAP, &store, &dict, true);
    assert_eq!(n, 0);
    assert_eq!(adaptive, fixed);
}
