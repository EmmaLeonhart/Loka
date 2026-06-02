//! Serverless-mode integration test: create a `.sdb`, no server, query it.
//!
//! This mirrors the embedded path that `loka query --data-dir` uses (and that
//! an SDK in serverless mode relies on): intern + insert into a
//! `PersistentStore`, flush, close, reopen, hydrate an in-memory store + term
//! dictionary, then run a SPARQL query against it. It verifies the full
//! serverless round-trip survives a disk close/reopen cycle — the existing
//! integration tests only exercise in-memory `TripleStore`s, and the
//! `loka-core` persistent tests cover quoted-triple provenance rather than the
//! user-facing query path.

use loka_core::{PersistentStore, TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

#[test]
fn serverless_sdb_roundtrip_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sdb");

    // Phase 1 — create the .sdb, intern IRIs, insert triples, flush, close.
    let alice;
    let knows;
    {
        let ps = PersistentStore::open(&path).unwrap();
        let alice_id = ps.intern("http://ex.org/alice").unwrap();
        let bob_id = ps.intern("http://ex.org/bob").unwrap();
        let carol_id = ps.intern("http://ex.org/carol").unwrap();
        let knows_id = ps.intern("http://ex.org/knows").unwrap();
        let name_id = ps.intern("http://ex.org/name").unwrap();
        let alice_name = ps.intern("\"Alice\"").unwrap();

        ps.insert(Triple::new(alice_id, knows_id, bob_id)).unwrap();
        ps.insert(Triple::new(alice_id, knows_id, carol_id))
            .unwrap();
        ps.insert(Triple::new(alice_id, name_id, alice_name))
            .unwrap();
        ps.flush().unwrap();

        alice = alice_id;
        knows = knows_id;
    } // store dropped — sled closed.

    // Phase 2 — reopen and hydrate an in-memory store + dict (the serverless query path).
    let ps = PersistentStore::open(&path).unwrap();
    let mut dict = TermDictionary::new();
    ps.load_terms_into(&mut dict);
    let mut store = TripleStore::new();
    let mut count = 0;
    for t in ps.iter() {
        store.insert(t).unwrap();
        count += 1;
    }
    assert_eq!(
        count, 3,
        "all three triples survived the close/reopen cycle"
    );

    // Interned ids round-trip through the rehydrated dictionary.
    assert_eq!(dict.lookup("http://ex.org/alice"), Some(alice));
    assert_eq!(dict.lookup("http://ex.org/knows"), Some(knows));

    // Phase 3 — run a SPARQL query against the reopened, hydrated store.
    let q =
        parse("SELECT ?who WHERE { <http://ex.org/alice> <http://ex.org/knows> ?who }").unwrap();
    let result = execute(&q, &store, &dict).unwrap();
    assert_eq!(result.rows.len(), 2, "alice knows two people");

    let bob = dict.lookup("http://ex.org/bob").unwrap();
    let carol = dict.lookup("http://ex.org/carol").unwrap();
    let found: Vec<_> = result.rows.iter().map(|r| *r.get("who").unwrap()).collect();
    assert!(found.contains(&bob), "bob is among alice's acquaintances");
    assert!(
        found.contains(&carol),
        "carol is among alice's acquaintances"
    );
}
