//! Equality in FILTER over the full term space.
//!
//! Filter comparison used to resolve only variables and integer literals,
//! returning `None` for everything else — so `FILTER(?n = "Ada")` compared
//! `Some(id)` against `None` and was always false. It matched nothing while the
//! identical literal in *pattern* position matched correctly.
//!
//! Every "string"/"iri"/"prefixed" case below returned 0 rows before the fix.

use loka_core::{TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

fn graph() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();

    let name = dict.intern("http://example.org/name");
    let city = dict.intern("http://example.org/city");
    let age = dict.intern("http://example.org/age");
    let london = dict.intern("http://example.org/London");
    let paris = dict.intern("http://example.org/Paris");

    for (who, nm, town, yrs) in [
        ("ada", "Ada", london, 36),
        ("bob", "Bob", paris, 25),
        ("cy", "Cy", london, 50),
    ] {
        let s = dict.intern(&format!("http://example.org/{}", who));
        store
            .insert(Triple::new(s, name, dict.intern(&format!("\"{}\"", nm))))
            .unwrap();
        store.insert(Triple::new(s, city, town)).unwrap();
        store
            .insert(Triple::new(s, age, loka_core::inline_integer(yrs).unwrap()))
            .unwrap();
    }
    (store, dict)
}

fn rows(patterns: &str, filter: &str) -> usize {
    let (store, dict) = graph();
    let q = format!(
        "PREFIX ex: <http://example.org/> SELECT ?s WHERE {{ {} FILTER({}) }}",
        patterns, filter
    );
    let parsed = parse(&q).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, q));
    execute(&parsed, &store, &dict)
        .unwrap_or_else(|e| panic!("did not execute: {}\n{}", e, q))
        .rows
        .len()
}

#[test]
fn string_literal_equality() {
    assert_eq!(rows("?s ex:name ?n .", r#"?n = "Ada""#), 1);
    assert_eq!(rows("?s ex:name ?n .", r#"?n = "Nobody""#), 0);
    assert_eq!(rows("?s ex:name ?n .", r#"?n != "Ada""#), 2);
}

#[test]
fn filter_agrees_with_pattern_position() {
    // The whole point: a term must mean the same thing in both places.
    let via_pattern = rows(r#"?s ex:name "Ada" ."#, "1 = 1");
    let via_filter = rows("?s ex:name ?n .", r#"?n = "Ada""#);
    assert_eq!(via_pattern, via_filter, "pattern and filter disagree");
    assert_eq!(via_filter, 1);
}

#[test]
fn iri_and_prefixed_name_equality() {
    assert_eq!(rows("?s ex:city ?c .", "?c = ex:London"), 2);
    assert_eq!(
        rows("?s ex:city ?c .", "?c = <http://example.org/London>"),
        2
    );
    assert_eq!(rows("?s ex:city ?c .", "?c = ex:Paris"), 1);
    assert_eq!(rows("?s ex:city ?c .", "?c != ex:London"), 1);
}

#[test]
fn unknown_terms_match_nothing_rather_than_erroring() {
    // A literal or IRI never ingested resolves to nothing; the row is excluded.
    assert_eq!(rows("?s ex:city ?c .", "?c = ex:Atlantis"), 0);
    assert_eq!(rows("?s ex:name ?n .", r#"?n = "Ghost""#), 0);
    // An unknown prefix is a non-match, not a hard error.
    assert_eq!(rows("?s ex:city ?c .", "?c = nosuch:Thing"), 0);
}

#[test]
fn integer_equality_unchanged() {
    // Regression guard on the path that already worked.
    assert_eq!(rows("?s ex:age ?a .", "?a = 36"), 1);
    assert_eq!(rows("?s ex:age ?a .", "?a != 36"), 2);
}

#[test]
fn ordering_still_works_on_integers() {
    assert_eq!(rows("?s ex:age ?a .", "?a > 30"), 2);
    assert_eq!(rows("?s ex:age ?a .", "?a < 30"), 1);
    assert_eq!(rows("?s ex:age ?a .", "?a >= 36"), 2);
    assert_eq!(rows("?s ex:age ?a .", "?a <= 25"), 1);
}

#[test]
fn ordering_on_strings_deliberately_matches_nothing() {
    // Ordering compares raw TermIds, which encode insertion order for
    // dictionary-interned strings — meaningless as a collation. Resolving
    // literals here would return an arbitrary subset, so the ordering path is
    // left narrow on purpose and a string comparison simply matches nothing.
    // Pinned so a future "fix" to widen it has to confront the choice.
    assert_eq!(rows("?s ex:name ?n .", r#"?n > "Ada""#), 0);
    assert_eq!(rows("?s ex:name ?n .", r#"?n < "Zed""#), 0);
}

#[test]
fn equality_composes_with_boolean_structure() {
    assert_eq!(
        rows(
            "?s ex:name ?n . ?s ex:city ?c .",
            r#"?n = "Ada" || ?n = "Cy""#
        ),
        2
    );
    assert_eq!(
        rows(
            "?s ex:name ?n . ?s ex:city ?c .",
            r#"(?n = "Ada" && ?c = ex:London) || ?n = "Bob""#
        ),
        2
    );
    assert_eq!(
        rows(
            "?s ex:name ?n . ?s ex:city ?c .",
            r#"(?n = "Ada" && ?c = ex:Paris) || ?n = "Bob""#
        ),
        1
    );
}
