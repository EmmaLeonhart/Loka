//! The zero-length step of `*` and `?` paths with an unbound subject. Until
//! 2026-10-11 it needed a bound subject: `?x p* ?y` returned nothing and
//! `?x p? ?y` only the one-step pairs. SPARQL 1.1 §18.5 (`ZeroLengthPath`):
//! with both ends free, every node of the graph (any subject or object)
//! pairs with itself; with the object bound, the subject binds to it.

use loka_core::{TermDictionary, TermId, Triple, TripleStore};
use loka_sparql::{execute, parse};
use std::collections::HashMap;

const IRI: &str = "http://loka.dev/";

/// a -p-> b, b -p-> b (a self-loop), c -q-> d; a is named "A".
/// Nodes: a, b, c, d, "A".
fn graph() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let p = dict.intern(&format!("{IRI}p"));
    let q = dict.intern(&format!("{IRI}q"));
    let name = dict.intern(&format!("{IRI}name"));
    let [a, b, c, d] = ["a", "b", "c", "d"].map(|n| dict.intern(&format!("{IRI}{n}")));
    let na = dict.intern("\"A\"");
    for (s, pr, o) in [(a, p, b), (b, p, b), (c, q, d), (a, name, na)] {
        store.insert(Triple::new(s, pr, o)).unwrap();
    }
    (store, dict)
}

fn run(q: &str) -> (Vec<HashMap<String, TermId>>, TermDictionary) {
    let (store, dict) = graph();
    let sparql = format!("PREFIX loka: <{IRI}> {q}");
    let parsed = parse(&sparql).unwrap_or_else(|e| panic!("{q}: {e}"));
    let rows = execute(&parsed, &store, &dict)
        .unwrap_or_else(|e| panic!("{q}: {e}"))
        .rows;
    (rows, dict)
}

fn local(dict: &TermDictionary, id: TermId) -> String {
    dict.resolve(id)
        .unwrap()
        .trim_start_matches(IRI)
        .to_string()
}

/// `(x, y)` local-name pairs, sorted.
fn pairs(q: &str) -> Vec<(String, String)> {
    let (rows, dict) = run(q);
    let mut out: Vec<_> = rows
        .iter()
        .map(|r| (local(&dict, r["x"]), local(&dict, r["y"])))
        .collect();
    out.sort();
    out
}

fn names(q: &str) -> Vec<String> {
    let (rows, dict) = run(q);
    let mut out: Vec<_> = rows.iter().map(|r| local(&dict, r["x"])).collect();
    out.sort();
    out
}

fn p(x: &str, y: &str) -> (String, String) {
    (x.to_string(), y.to_string())
}

#[test]
fn star_with_both_ends_free_pairs_every_node_with_itself_plus_the_walk() {
    // b reaches b by zero steps and by the loop: one row, not two.
    assert_eq!(
        pairs("SELECT ?x ?y WHERE { ?x loka:p* ?y }"),
        [
            p("\"A\"", "\"A\""),
            p("a", "a"),
            p("a", "b"),
            p("b", "b"),
            p("c", "c"),
            p("d", "d"),
        ]
    );
}

#[test]
fn star_with_a_bound_object_walks_backwards_and_includes_the_object() {
    assert_eq!(
        names("SELECT ?x WHERE { ?x loka:p* <http://loka.dev/b> }"),
        ["a", "b"]
    );
    assert_eq!(
        names("SELECT ?x WHERE { ?x loka:p* <http://loka.dev/a> }"),
        ["a"]
    );
    // d has no p edges at all; zero steps still reach it.
    assert_eq!(
        names("SELECT ?x WHERE { ?x loka:p* <http://loka.dev/d> }"),
        ["d"]
    );
}

#[test]
fn star_with_the_same_variable_at_both_ends_is_one_row_per_node() {
    assert_eq!(
        names("SELECT ?x WHERE { ?x loka:p* ?x }"),
        ["\"A\"", "a", "b", "c", "d"]
    );
}

#[test]
fn plus_is_unchanged_it_has_no_zero_step() {
    assert_eq!(
        pairs("SELECT ?x ?y WHERE { ?x loka:p+ ?y }"),
        [p("a", "b"), p("b", "b")]
    );
}

#[test]
fn question_mark_with_a_bound_object() {
    assert_eq!(
        names("SELECT ?x WHERE { ?x loka:p? <http://loka.dev/b> }"),
        ["a", "b"]
    );
    assert_eq!(
        names("SELECT ?x WHERE { ?x loka:p? <http://loka.dev/a> }"),
        ["a"]
    );
}

#[test]
fn question_mark_with_both_ends_free() {
    assert_eq!(
        pairs("SELECT ?x ?y WHERE { ?x loka:p? ?y }"),
        [
            p("\"A\"", "\"A\""),
            p("a", "a"),
            p("a", "b"),
            p("b", "b"),
            p("c", "c"),
            p("d", "d"),
        ]
    );
}

#[test]
fn a_join_that_binds_the_subject_first_is_unaffected() {
    let q = r#"SELECT ?x ?y WHERE { ?x loka:name "A" . ?x loka:p* ?y }"#;
    assert_eq!(pairs(q), [p("a", "a"), p("a", "b")]);
}
