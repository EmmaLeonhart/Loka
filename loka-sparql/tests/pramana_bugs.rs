//! Two bugs found dogfooding Pramana on Loka (2026-07-20, TODO.md).

use loka_core::{TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

const P: &str = "http://pramana.org/prop/direct/";
const E: &str = "http://pramana.org/entity/";

fn store() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let label = dict.intern(&format!("{P}EntityLabel"));
    let uuid = dict.intern(&format!("{P}uuid"));
    let subject = dict.intern(&format!("{P}subject"));
    let e1 = dict.intern(&format!("{E}3946bf48"));
    let lit_label = dict.intern("\"GAP2-timing-probe\"");
    let lit_uuid = dict.intern("\"3946bf48-aaaa\"");
    store.insert(Triple::new(e1, label, lit_label)).unwrap();
    store.insert(Triple::new(e1, uuid, lit_uuid)).unwrap();
    for k in 0..5 {
        let prop = dict.intern(&format!("{E}prop{k}"));
        store.insert(Triple::new(prop, subject, e1)).unwrap();
    }
    (store, dict)
}

fn count(q: &str) -> usize {
    let (store, dict) = store();
    execute(&parse(q).unwrap(), &store, &dict)
        .unwrap()
        .rows
        .len()
}

#[test]
fn prefixed_predicate_with_literal_object_matches() {
    let full = count(&format!(
        "SELECT ?e WHERE {{ ?e <{P}EntityLabel> \"GAP2-timing-probe\" }}"
    ));
    let prefixed = count(&format!(
        "PREFIX wdt: <{P}> SELECT ?e WHERE {{ ?e wdt:EntityLabel \"GAP2-timing-probe\" }}"
    ));
    assert_eq!(full, 1);
    assert_eq!(
        prefixed, 1,
        "prefixed predicate must match like the full IRI"
    );
}

#[test]
fn object_variable_joins_into_a_literal_bound_pattern() {
    let each_leg_a = count(&format!("SELECT ?p ?s WHERE {{ ?p <{P}subject> ?s }}"));
    let each_leg_b = count(&format!(
        "SELECT ?s WHERE {{ ?s <{P}uuid> \"3946bf48-aaaa\" }}"
    ));
    assert_eq!((each_leg_a, each_leg_b), (5, 1));
    let joined = count(&format!(
        "SELECT ?p WHERE {{ ?p <{P}subject> ?s . ?s <{P}uuid> \"3946bf48-aaaa\" }}"
    ));
    assert_eq!(joined, 5);
    let reversed = count(&format!(
        "SELECT ?p WHERE {{ ?s <{P}uuid> \"3946bf48-aaaa\" . ?p <{P}subject> ?s }}"
    ));
    assert_eq!(reversed, 5);
}

#[test]
fn the_planner_picks_the_same_order_every_time() {
    // The July addendum suspected hash-seeded planner order made the join
    // nondeterministic across processes. Each fresh store and dictionary
    // gets new HashMap seeds; the plan must not change with them. (Ten
    // fresh `loka serve` processes on one store also returned identical
    // rows, 2026-10-08.)
    let q = format!(
        "PREFIX wdt: <{P}> SELECT ?p ?l WHERE {{ ?p wdt:subject ?s . \
         ?s wdt:uuid \"3946bf48-aaaa\" . ?s wdt:EntityLabel ?l }}"
    );
    let mut orders = std::collections::BTreeSet::new();
    for _ in 0..50 {
        let (store, dict) = store();
        let mut parsed = parse(&q).unwrap();
        loka_sparql::optimize_full(&mut parsed, Some(&store), Some(&dict));
        orders.insert(format!("{:?}", parsed.patterns));
        assert_eq!(execute(&parsed, &store, &dict).unwrap().rows.len(), 5);
    }
    assert_eq!(
        orders.len(),
        1,
        "one plan across 50 differently seeded runs"
    );
}
