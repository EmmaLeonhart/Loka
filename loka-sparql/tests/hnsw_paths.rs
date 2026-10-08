//! Property paths over the virtual HNSW edge predicates, and distinct results
//! for `+` / `*` paths.
//!
//! HNSW edges are virtual: answered from the live index, never stored. Until
//! 2026-10-07 the `+`/`*` traversal only walked stored triples, so
//! `loka:hnswNeighbor+` reached nothing.

use std::collections::{BTreeSet, HashSet};

use loka_core::{DatabaseConfig, HnswEdgeMode, TermDictionary, TermId, Triple, TripleStore};
use loka_hnsw::{DistanceMetric, VectorPredicateConfig, VectorRegistry};
use loka_sparql::{execute, execute_with_config, parse};

const EMB: &str = "http://example.org/hasEmbedding";

/// Eight documents with 3-d embeddings in a small HNSW index.
fn indexed() -> (TripleStore, TermDictionary, VectorRegistry, Vec<TermId>) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let emb = dict.intern(EMB);
    dict.intern(loka_hnsw::HNSW_NEIGHBOR_IRI);
    let mut vectors = VectorRegistry::new();
    vectors
        .declare(VectorPredicateConfig {
            predicate_id: emb,
            dimensions: 3,
            m: 4,
            ef_construction: 20,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();
    let points = [
        [1.0, 0.0, 0.0],
        [0.9, 0.1, 0.0],
        [0.8, 0.2, 0.1],
        [0.0, 1.0, 0.0],
        [0.1, 0.9, 0.1],
        [0.0, 0.0, 1.0],
        [0.1, 0.1, 0.9],
        [0.5, 0.5, 0.5],
    ];
    let mut docs = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let doc = dict.intern(&format!("http://example.org/doc{i}"));
        let v = dict.intern(&format!("\"v{i}\"^^<http://loka.dev/f32vec>"));
        store.insert(Triple::new(doc, emb, v)).unwrap();
        vectors.insert(emb, p.to_vec(), v).unwrap();
        docs.push(doc);
    }
    (store, dict, vectors, docs)
}

fn virtual_config() -> DatabaseConfig {
    DatabaseConfig {
        hnsw_edge_mode: HnswEdgeMode::Virtual,
        ..Default::default()
    }
}

/// `?n` values of a query, in result order.
fn ns(
    query: &str,
    store: &TripleStore,
    dict: &TermDictionary,
    vectors: &VectorRegistry,
) -> Vec<TermId> {
    let q = parse(query).unwrap();
    execute_with_config(&q, store, dict, vectors, &virtual_config())
        .unwrap()
        .rows
        .iter()
        .map(|r| *r.get("n").unwrap())
        .collect()
}

fn iri(dict: &TermDictionary, id: TermId) -> String {
    dict.resolve(id).unwrap().to_string()
}

#[test]
fn bound_source_single_hop_returns_neighbours() {
    let (store, dict, vectors, docs) = indexed();
    let got = ns(
        &format!(
            "SELECT ?n WHERE {{ <{}> <{}> ?n }}",
            iri(&dict, docs[0]),
            loka_hnsw::HNSW_NEIGHBOR_IRI
        ),
        &store,
        &dict,
        &vectors,
    );
    assert!(!got.is_empty(), "doc0 has HNSW neighbours");
    assert!(got.iter().all(|n| docs.contains(n) && *n != docs[0]));
}

#[test]
fn one_or_more_path_reaches_the_component_reached_hop_by_hop() {
    let (store, dict, vectors, docs) = indexed();
    // Reference: BFS on the client side using single-hop queries only.
    let mut reached: BTreeSet<TermId> = BTreeSet::new();
    let mut frontier = vec![docs[0]];
    let mut seen: HashSet<TermId> = HashSet::new();
    while let Some(node) = frontier.pop() {
        if !seen.insert(node) {
            continue;
        }
        for n in ns(
            &format!(
                "SELECT ?n WHERE {{ <{}> <{}> ?n }}",
                iri(&dict, node),
                loka_hnsw::HNSW_NEIGHBOR_IRI
            ),
            &store,
            &dict,
            &vectors,
        ) {
            reached.insert(n);
            frontier.push(n);
        }
    }
    assert!(reached.len() > 1, "the reference walk goes beyond one hop");

    let path = ns(
        &format!(
            "SELECT ?n WHERE {{ <{}> <{}>+ ?n }}",
            iri(&dict, docs[0]),
            loka_hnsw::HNSW_NEIGHBOR_IRI
        ),
        &store,
        &dict,
        &vectors,
    );
    let path_set: BTreeSet<TermId> = path.iter().copied().collect();
    assert_eq!(path_set, reached);
    assert_eq!(path.len(), path_set.len(), "each node once");
}

#[test]
fn one_or_more_on_stored_triples_yields_each_node_once() {
    // Diamond: a -> b, a -> c, b -> d, c -> d. `d` is reached by two edges.
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let p = dict.intern("http://example.org/p");
    let id = |dict: &mut TermDictionary, n: &str| dict.intern(&format!("http://example.org/{n}"));
    let (a, b, c, d) = (
        id(&mut dict, "a"),
        id(&mut dict, "b"),
        id(&mut dict, "c"),
        id(&mut dict, "d"),
    );
    for (s, o) in [(a, b), (a, c), (b, d), (c, d)] {
        store.insert(Triple::new(s, p, o)).unwrap();
    }
    let q = parse("SELECT ?n WHERE { <http://example.org/a> <http://example.org/p>+ ?n }").unwrap();
    let rows = execute(&q, &store, &dict).unwrap().rows;
    let got: Vec<TermId> = rows.iter().map(|r| *r.get("n").unwrap()).collect();
    let set: BTreeSet<TermId> = got.iter().copied().collect();
    assert_eq!(set, BTreeSet::from([b, c, d]));
    assert_eq!(got.len(), 3, "d once, not once per incoming edge");
}

#[test]
fn documented_descend_then_horizontal_path_runs() {
    // The executor's own doc example: descend layers, then walk horizontally.
    let (store, dict, vectors, docs) = indexed();
    let got = ns(
        &format!(
            "SELECT ?n WHERE {{ <{}> <{}>*/<{}>+ ?n }}",
            iri(&dict, docs[0]),
            loka_hnsw::HNSW_LAYER_DESCEND_IRI,
            loka_hnsw::HNSW_HORIZONTAL_NEIGHBOR_IRI
        ),
        &store,
        &dict,
        &vectors,
    );
    assert!(!got.is_empty(), "reaches horizontal neighbours");
    assert!(got.iter().all(|n| docs.contains(n)));
}

#[test]
fn three_step_sequence_joins_through_each_intermediate() {
    // a -p-> b -q-> c -r-> d, plus a decoy x -q-> y -r-> z.
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let e = |dict: &mut TermDictionary, n: &str| dict.intern(&format!("http://example.org/{n}"));
    let (p, q, r) = (e(&mut dict, "p"), e(&mut dict, "q"), e(&mut dict, "r"));
    let (a, b, c, d) = (
        e(&mut dict, "a"),
        e(&mut dict, "b"),
        e(&mut dict, "c"),
        e(&mut dict, "d"),
    );
    let (x, y, z) = (e(&mut dict, "x"), e(&mut dict, "y"), e(&mut dict, "z"));
    for (s, pr, o) in [(a, p, b), (b, q, c), (c, r, d), (x, q, y), (y, r, z)] {
        store.insert(Triple::new(s, pr, o)).unwrap();
    }
    let qy = parse(
        "SELECT * WHERE { <http://example.org/a> \
         <http://example.org/p>/<http://example.org/q>/<http://example.org/r> ?n }",
    )
    .unwrap();
    let result = execute(&qy, &store, &dict).unwrap();
    let got: Vec<TermId> = result.rows.iter().map(|r| *r.get("n").unwrap()).collect();
    assert_eq!(got, vec![d]);
    // Intermediate path variables are internal: SELECT * shows only ?n.
    assert_eq!(result.columns, vec!["n".to_string()]);
}
