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
