//! Computed values, stage 4: `SELECT (expr AS ?v)` and `ORDER BY expr`, and
//! ORDER BY comparing VALUES rather than term ids.
//!
//! Term ids are handed out in first-seen order, and computed ids in
//! first-computed order, so a sort on ids orders strings by when they were
//! stored. Every test below inserts data in the opposite order to the expected
//! sort, so an id-based sort fails them.

use loka_core::{TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

const NAME: &str = "http://ex.org/name";

/// Three people whose names are interned in reverse alphabetical order.
fn people() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let name = dict.intern(NAME);
    for (who, label) in [("c", "\"carol\""), ("b", "\"Bob\""), ("a", "\"alice\"")] {
        let s = dict.intern(&format!("http://ex.org/{who}"));
        let o = dict.intern(label);
        store.insert(Triple::new(s, name, o)).unwrap();
    }
    (store, dict)
}

fn names_in_order(
    rows: &[std::collections::HashMap<String, u64>],
    dict: &TermDictionary,
) -> Vec<String> {
    rows.iter()
        .map(|r| dict.resolve(*r.get("n").unwrap()).unwrap().to_string())
        .collect()
}

#[test]
fn order_by_variable_sorts_strings_by_value_not_insertion() {
    let (store, dict) = people();
    let q = parse(&format!("SELECT ?n WHERE {{ ?s <{NAME}> ?n }} ORDER BY ?n")).unwrap();
    let result = execute(&q, &store, &dict).unwrap();
    // Codepoint order: uppercase before lowercase.
    assert_eq!(
        names_in_order(&result.rows, &dict),
        vec!["\"Bob\"", "\"alice\"", "\"carol\""]
    );
    let q = parse(&format!(
        "SELECT ?n WHERE {{ ?s <{NAME}> ?n }} ORDER BY DESC(?n)"
    ))
    .unwrap();
    let result = execute(&q, &store, &dict).unwrap();
    assert_eq!(
        names_in_order(&result.rows, &dict),
        vec!["\"carol\"", "\"alice\"", "\"Bob\""]
    );
}

#[test]
fn select_expression_as_variable_projects_the_computed_value() {
    let (store, dict) = people();
    let q = parse(&format!(
        "SELECT ?s (UCASE(?n) AS ?shout) WHERE {{ ?s <{NAME}> ?n }} ORDER BY ?shout"
    ))
    .unwrap();
    assert_eq!(q.projection, vec!["s".to_string(), "shout".to_string()]);
    let result = execute(&q, &store, &dict).unwrap();
    let shouts: Vec<&str> = result
        .rows
        .iter()
        .map(|r| result.values.get(*r.get("shout").unwrap()).unwrap())
        .collect();
    assert_eq!(shouts, vec!["ALICE", "BOB", "CAROL"]);
}

#[test]
fn order_by_expression_compares_the_computed_string() {
    let (store, dict) = people();
    // LCASE makes the order case-insensitive; the keys are computed in
    // reverse-alphabetical order, so sorting by computed id would fail.
    for (clause, expected) in [
        (
            "ORDER BY LCASE(?n)",
            vec!["\"alice\"", "\"Bob\"", "\"carol\""],
        ),
        (
            "ORDER BY ASC(LCASE(?n))",
            vec!["\"alice\"", "\"Bob\"", "\"carol\""],
        ),
        (
            "ORDER BY DESC(LCASE(?n))",
            vec!["\"carol\"", "\"Bob\"", "\"alice\""],
        ),
    ] {
        let q = parse(&format!("SELECT ?n WHERE {{ ?s <{NAME}> ?n }} {clause}")).unwrap();
        let result = execute(&q, &store, &dict).unwrap();
        assert_eq!(names_in_order(&result.rows, &dict), expected, "{clause}");
    }
}

#[test]
fn order_by_expression_key_is_not_projected_by_select_star() {
    let (store, dict) = people();
    let q = parse(&format!(
        "SELECT * WHERE {{ ?s <{NAME}> ?n }} ORDER BY LCASE(?n)"
    ))
    .unwrap();
    let result = execute(&q, &store, &dict).unwrap();
    assert_eq!(result.columns, vec!["n".to_string(), "s".to_string()]);
}

#[test]
fn order_by_numeric_typed_literals_compares_numbers() {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let p = dict.intern("http://ex.org/score");
    // As strings, "10.5" < "9.25"; as numbers it is the other way round.
    for (who, v) in [("x", "10.5"), ("y", "9.25"), ("z", "100.0")] {
        let s = dict.intern(&format!("http://ex.org/{who}"));
        let o = dict.intern(&format!(
            "\"{v}\"^^<http://www.w3.org/2001/XMLSchema#decimal>"
        ));
        store.insert(Triple::new(s, p, o)).unwrap();
    }
    let q = parse("SELECT ?s ?v WHERE { ?s <http://ex.org/score> ?v } ORDER BY ?v").unwrap();
    let result = execute(&q, &store, &dict).unwrap();
    let subjects: Vec<&str> = result
        .rows
        .iter()
        .map(|r| dict.resolve(*r.get("s").unwrap()).unwrap())
        .collect();
    assert_eq!(
        subjects,
        vec!["http://ex.org/y", "http://ex.org/x", "http://ex.org/z"]
    );
}
