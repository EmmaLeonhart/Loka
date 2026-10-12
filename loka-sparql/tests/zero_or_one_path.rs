//! SPARQL `?` (zero-or-one) property paths. Until 2026-10-11 the parser never
//! produced one, so `loka:p?` didn't parse, and the executor's zero step
//! overwrote an already-bound object instead of checking it.

use loka_core::{TermDictionary, TermId, Triple, TripleStore};
use loka_sparql::{execute, parse};
use std::collections::HashMap;

const IRI: &str = "http://loka.dev/";

/// a -p-> b, b -p-> b (a self-loop); a is named "A", b "B".
fn graph() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let p = dict.intern(&format!("{IRI}p"));
    let name = dict.intern(&format!("{IRI}name"));
    let a = dict.intern(&format!("{IRI}a"));
    let b = dict.intern(&format!("{IRI}b"));
    let (na, nb) = (dict.intern("\"A\""), dict.intern("\"B\""));
    for (s, pr, o) in [(a, p, b), (b, p, b), (a, name, na), (b, name, nb)] {
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

/// The local names bound to `var`, sorted.
fn names(q: &str, var: &str) -> Vec<String> {
    let (rows, dict) = run(q);
    let mut out: Vec<String> = rows
        .iter()
        .map(|r| {
            dict.resolve(r[var])
                .unwrap()
                .trim_start_matches(IRI)
                .to_string()
        })
        .collect();
    out.sort();
    out
}

#[test]
fn zero_or_one_step_from_a_bound_subject() {
    assert_eq!(
        names("SELECT ?o WHERE { <http://loka.dev/a> loka:p? ?o }", "o"),
        ["a", "b"]
    );
}

#[test]
fn a_self_loop_is_one_row_not_two() {
    // b reaches b by zero steps and by the loop; `?` yields the pair once.
    assert_eq!(
        names("SELECT ?o WHERE { <http://loka.dev/b> loka:p? ?o }", "o"),
        ["b"]
    );
}

#[test]
fn a_constant_object_is_checked_on_the_zero_step() {
    let q = r#"SELECT ?x WHERE { ?x loka:name "A" . ?x loka:p? <http://loka.dev/a> }"#;
    assert_eq!(names(q, "x"), ["a"]);
    let q = r#"SELECT ?x WHERE { ?x loka:name "A" . ?x loka:p? <http://loka.dev/b> }"#;
    assert_eq!(names(q, "x"), ["a"]);
    let q = r#"SELECT ?x WHERE { ?x loka:name "B" . ?x loka:p? <http://loka.dev/a> }"#;
    assert!(names(q, "x").is_empty());
}

#[test]
fn a_bound_object_is_checked_not_overwritten() {
    let q = r#"SELECT ?y WHERE { ?y loka:name "B" . <http://loka.dev/a> loka:p? ?y }"#;
    assert_eq!(names(q, "y"), ["b"]);
}

#[test]
fn a_question_mark_starting_a_variable_is_the_object() {
    // `loka:p?o` is `loka:p ?o`, one step only, as SPARQL tokenizes it.
    assert_eq!(
        names("SELECT ?o WHERE { <http://loka.dev/a> loka:p?o }", "o"),
        ["b"]
    );
}

#[test]
fn an_unbound_subject_gets_every_node_and_the_one_step_pairs() {
    // Zero step (SPARQL 1.1 §18.5): every node pairs with itself, so a, b,
    // "A", "B". One step adds a→b; b's self-loop is already the zero row.
    // Until 2026-10-11 the zero step needed a bound subject and this gave 2.
    let (rows, _) = run("SELECT ?x ?y WHERE { ?x loka:p? ?y }");
    assert_eq!(rows.len(), 5, "{rows:?}");
}
