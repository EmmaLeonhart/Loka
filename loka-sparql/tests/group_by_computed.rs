//! Computed values, stage 5: GROUP BY on a computed value.
//!
//! The motivating query is Pramana's type count: count items by the LOCAL NAME
//! of their type. Two different type IRIs can share a local name, so grouping
//! on the full IRI (what Pramana does today, folding names client-side) splits
//! what should be one group.

use std::collections::HashMap;

use loka_core::{decode_inline_integer, TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Items typed with two different `.../Entity` IRIs and one `.../Thing` IRI.
fn typed() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let ty = dict.intern(RDF_TYPE);
    for (item, class) in [
        ("i1", "http://a.org/onto/Entity"),
        ("i2", "http://b.org/schema/Entity"),
        ("i3", "http://b.org/schema/Entity"),
        ("i4", "http://a.org/onto/Thing"),
    ] {
        let s = dict.intern(&format!("http://ex.org/{item}"));
        let o = dict.intern(class);
        store.insert(Triple::new(s, ty, o)).unwrap();
    }
    (store, dict)
}

/// local name -> count, read through the result's value table.
fn counts(query: &str) -> HashMap<String, i64> {
    let (store, dict) = typed();
    let q = parse(query).unwrap();
    let result = execute(&q, &store, &dict).unwrap();
    result
        .rows
        .iter()
        .map(|r| {
            let t = result
                .values
                .get(*r.get("t").expect("?t bound"))
                .expect("computed")
                .to_string();
            let n = decode_inline_integer(*r.get("n").expect("?n bound")).unwrap();
            (t, n)
        })
        .collect()
}

#[test]
fn group_by_expression_as_variable_merges_equal_computed_values() {
    let got = counts(&format!(
        "SELECT ?t (COUNT(?item) AS ?n) WHERE {{ ?item <{RDF_TYPE}> ?type }} \
         GROUP BY (REPLACE(STR(?type), \"^.*/\", \"\") AS ?t)"
    ));
    assert_eq!(
        got,
        HashMap::from([("Entity".to_string(), 3), ("Thing".to_string(), 1)])
    );
}

#[test]
fn group_by_full_iri_still_splits_them() {
    // The control: grouping on the IRI gives three groups, which is why the
    // computed key is needed.
    let (store, dict) = typed();
    let q = parse(&format!(
        "SELECT ?type (COUNT(?item) AS ?n) WHERE {{ ?item <{RDF_TYPE}> ?type }} GROUP BY ?type"
    ))
    .unwrap();
    assert_eq!(execute(&q, &store, &dict).unwrap().rows.len(), 3);
}

#[test]
fn group_by_bare_expression_groups_under_a_hidden_key() {
    let (store, dict) = typed();
    let q = parse(&format!(
        "SELECT (COUNT(?item) AS ?n) WHERE {{ ?item <{RDF_TYPE}> ?type }} \
         GROUP BY REPLACE(STR(?type), \"^.*/\", \"\")"
    ))
    .unwrap();
    let result = execute(&q, &store, &dict).unwrap();
    let mut ns: Vec<i64> = result
        .rows
        .iter()
        .map(|r| decode_inline_integer(*r.get("n").unwrap()).unwrap())
        .collect();
    ns.sort();
    assert_eq!(ns, vec![1, 3]);
}
