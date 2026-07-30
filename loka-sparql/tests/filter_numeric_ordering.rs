//! Numeric ordering in FILTER, including negatives and arithmetic operands.
//!
//! Two defects live here, both wrong-answer rather than parse-error shaped:
//!
//! 1. **Negative integers ordered wrongly.** Ordering compared raw `TermId`s.
//!    An inline integer's payload is two's-complement in the low 56 bits, so a
//!    negative value sets the payload's high bit and the *unsigned* id sorts
//!    above every positive one — `FILTER(?t > -5)` excluded rows it should keep.
//! 2. **Arithmetic operands were parsed and discarded.** `FILTER(?t + 5 > 30)`
//!    parsed, dropped the `+ 5`, and evaluated `FILTER(?t > 30)`.

use loka_core::{TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

/// Temperatures spanning zero, so sign handling is load-bearing: -20, -5, 0, 5, 20.
fn temps() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let temp = dict.intern("http://example.org/temp");
    for (i, t) in [-20i64, -5, 0, 5, 20].iter().enumerate() {
        let s = dict.intern(&format!("http://example.org/r{}", i));
        store
            .insert(Triple::new(s, temp, loka_core::inline_integer(*t).unwrap()))
            .unwrap();
    }
    (store, dict)
}

fn rows(filter: &str) -> usize {
    let (store, dict) = temps();
    let q = format!(
        "PREFIX ex: <http://example.org/> \
         SELECT ?s WHERE {{ ?s ex:temp ?t . FILTER({}) }}",
        filter
    );
    let parsed = parse(&q).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, q));
    execute(&parsed, &store, &dict)
        .unwrap_or_else(|e| panic!("did not execute: {}\n{}", e, q))
        .rows
        .len()
}

#[test]
fn ordering_handles_negative_integers() {
    // -20, -5, 0, 5, 20
    assert_eq!(rows("?t > -5"), 3); // 0, 5, 20
    assert_eq!(rows("?t >= -5"), 4); // -5, 0, 5, 20
    assert_eq!(rows("?t < 0"), 2); // -20, -5
    assert_eq!(rows("?t <= -5"), 2); // -20, -5
    assert_eq!(rows("?t > -100"), 5);
    assert_eq!(rows("?t < -100"), 0);
    // Both operands negative.
    assert_eq!(rows("?t < -6"), 1); // -20
                                    // A negative bound must not silently outrank the positives, which is what
                                    // the raw-id comparison did.
    assert_eq!(rows("?t > -21 && ?t < 21"), 5);
}

#[test]
fn ordering_still_correct_for_positive_integers() {
    assert_eq!(rows("?t > 4"), 2); // 5, 20
    assert_eq!(rows("?t >= 5"), 2);
    assert_eq!(rows("?t < 5"), 3); // -20, -5, 0
    assert_eq!(rows("?t <= 0"), 3);
    assert_eq!(rows("?t = 0"), 1);
    assert_eq!(rows("?t != 0"), 4);
}

#[test]
fn arithmetic_in_the_left_operand_is_evaluated() {
    // ?t + 25 over -20,-5,0,5,20 -> 5, 20, 25, 30, 45
    assert_eq!(rows("?t + 25 > 24"), 3); // 25, 30, 45
    assert_eq!(rows("?t + 25 >= 25"), 3); // 25, 30, 45
    assert_eq!(rows("?t - 5 > 0"), 1); // 20 only
    assert_eq!(rows("?t * 2 = 40"), 1); // 20
    assert_eq!(rows("?t * 2 = 41"), 0);
    assert_eq!(rows("?t / 5 = 4"), 1); // 20
    assert_eq!(rows("?t + 0 = 0"), 1);
    // The pre-fix behaviour dropped the arithmetic, so `?t + 25 > 24` evaluated
    // as `?t > 24` and matched nothing.
    assert_ne!(rows("?t + 25 > 24"), 0);
}

/// Division is real division, not truncating integer division: `5 / 2` is 2.5,
/// so it does not equal 2. Truncation would make this 1 row.
#[test]
fn division_is_not_truncated() {
    assert_eq!(rows("?t / 2 = 2"), 0);
    assert_eq!(rows("?t / 2 = 10"), 1); // 20 / 2
    assert_eq!(rows("?t / 4 = -5"), 1); // -20 / 4
}

#[test]
fn arithmetic_in_the_right_operand_is_evaluated() {
    // Previously a parse error: the right operand accepted a bare term only.
    assert_eq!(rows("24 < ?t + 25"), 3);
    assert_eq!(rows("0 > ?t - 5"), 3); // -25, -10, -5 (0 > 0 is false)
    assert_eq!(rows("40 = ?t * 2"), 1);
}

#[test]
fn arithmetic_composes_with_boolean_structure() {
    // ?t + 25 > 24 holds for t = 0, 5, 20.
    assert_eq!(rows("?t + 25 > 24 && ?t < 20"), 2); // 0, 5
    assert_eq!(rows("?t + 25 > 100 || ?t = 0"), 1); // nothing exceeds 100
    assert_eq!(rows("(?t + 25 > 24) && (?t - 5 < 0)"), 1); // t < 5 -> 0
    assert_eq!(rows("!(?t + 25 > 24)"), 2); // -20, -5
}

/// Arithmetic has no operator precedence yet: `?t + 2 * 3` is `(?t + 2) * 3`,
/// where SPARQL binds `*` tighter and means `?t + (2 * 3)`. Pinned rather than
/// asserted-correct, the same way `&&`/`||` precedence was pinned before it was
/// fixed — see TODO.md.
#[test]
fn arithmetic_has_no_operator_precedence_yet() {
    // (t + 2) * 3 over -20,-5,0,5,20 -> -54, -9, 6, 21, 66; = 6 picks t = 0.
    assert_eq!(rows("?t + 2 * 3 = 6"), 1);
    // SPARQL's reading, t + 6 = 6, would also pick t = 0 — so use a case where
    // the two differ: (t + 2) * 3 = 21 picks t = 5; t + (2 * 3) = 21 picks
    // t = 15, which is not in the store.
    assert_eq!(rows("?t + 2 * 3 = 21"), 1);
    assert_eq!(rows("?t + 2 * 3 = 26"), 0);
    // Explicit intent is expressible either way by writing the constant folded.
    assert_eq!(rows("?t + 6 = 21"), 0);
    assert_eq!(rows("?t + 6 = 11"), 1); // t = 5
}

#[test]
fn arithmetic_on_non_numeric_operands_matches_nothing() {
    // Matching how the rest of filter evaluation treats a term it cannot
    // resolve: no match, rather than an error or an arbitrary answer.
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let name = dict.intern("http://example.org/name");
    let s = dict.intern("http://example.org/a");
    store
        .insert(Triple::new(s, name, dict.intern("\"Ada\"")))
        .unwrap();

    for f in [r#"?n + 1 > 0"#, r#"?n * 2 = 4"#, r#"?n + 1 = "Ada""#] {
        let q = format!(
            "PREFIX ex: <http://example.org/> \
             SELECT ?s WHERE {{ ?s ex:name ?n . FILTER({}) }}",
            f
        );
        let parsed = parse(&q).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, q));
        assert_eq!(
            execute(&parsed, &store, &dict).unwrap().rows.len(),
            0,
            "{}",
            q
        );
    }
}

/// Division by zero has no value, so the comparison is false — not a panic and
/// not a wrong row.
#[test]
fn division_by_zero_matches_nothing_rather_than_panicking() {
    assert_eq!(rows("?t / 0 = 0"), 0);
    assert_eq!(rows("?t / 0 > -100"), 0);
}
