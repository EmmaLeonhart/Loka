//! Per-query computed values: the id space, in-query interning, and the one
//! invariant that matters — a computed id must never reach storage.
//!
//! Stage 1 of `planning/computed-values.md`. Nothing produces these ids yet;
//! this pins the contract they will be produced against, and in particular pins
//! the storage rejection **before** `INSERT … WHERE` exists, since that is the
//! feature that would otherwise introduce the hazard quietly.

use loka_core::{
    decode_inline_boolean, decode_inline_integer, inline_boolean, inline_integer, inline_type,
    is_computed, is_inline, CoreError, InlineType, PersistentStore, QueryValues, TermDictionary,
    Triple, TripleStore,
};

#[test]
fn interning_is_by_value_within_the_query() {
    let mut values = QueryValues::new();
    let a = values.intern("Entity").unwrap();
    let b = values.intern("Entity").unwrap();
    let c = values.intern("Property").unwrap();

    // Equal strings MUST get equal ids, or DISTINCT / GROUP BY / joins on a
    // computed variable would treat identical values as different.
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(values.len(), 2);
    assert_eq!(values.get(a), Some("Entity"));
    assert_eq!(values.get(c), Some("Property"));
}

#[test]
fn a_computed_id_is_inline_and_tagged_as_computed() {
    let mut values = QueryValues::new();
    let id = values.intern("Entity").unwrap();

    assert!(is_inline(id), "must not look like a dictionary pointer");
    assert!(is_computed(id));
    assert_eq!(inline_type(id), Some(InlineType::Computed));
}

/// Every existing decoder checks its own tag, so an unknown one reads as "no
/// value" rather than as a wrong value. That is what makes the new tag safe to
/// add ahead of the code that consumes it.
#[test]
fn existing_decoders_decline_a_computed_id() {
    let mut values = QueryValues::new();
    let id = values.intern("Entity").unwrap();

    assert_eq!(decode_inline_integer(id), None);
    assert_eq!(decode_inline_boolean(id), None);
    assert_eq!(loka_core::decode_inline_temporal(id), None);

    // And the dictionary does not claim it either.
    let dict = TermDictionary::new();
    assert_eq!(dict.resolve(id), None);
}

#[test]
fn computed_ids_do_not_collide_with_dictionary_or_other_inline_ids() {
    let mut dict = TermDictionary::new();
    let mut values = QueryValues::new();

    let mut minted = Vec::new();
    let mut dict_ids = Vec::new();
    for i in 0..200 {
        minted.push(values.intern(&format!("v{}", i)).unwrap());
        dict_ids.push(dict.intern(&format!("http://example.org/{}", i)));
    }

    // No computed id is a dictionary id, in either direction. Dictionary
    // pointers have bit 63 clear and computed ids have it set, so this holds by
    // construction — the test is here so a change to the id layout has to break
    // something.
    for &id in &minted {
        assert!(!dict_ids.contains(&id));
        assert_eq!(dict.resolve(id), None);
        assert!(decode_inline_integer(id).is_none());
    }
    for &id in &dict_ids {
        assert!(!is_computed(id));
        assert!(!is_inline(id));
    }

    // Distinct from the other inline types too.
    assert!(!is_computed(inline_integer(42).unwrap()));
    assert!(!is_computed(inline_integer(-42).unwrap()));
    assert!(!is_computed(inline_boolean(true)));
}

#[test]
fn get_declines_ids_it_did_not_mint() {
    let mut values = QueryValues::new();
    let id = values.intern("Entity").unwrap();

    let mut other = QueryValues::new();
    // A *shorter* table: the same id indexes past its end, so it must decline
    // rather than return a neighbour's value. This is the corruption shape the
    // storage rejection below exists to prevent, demonstrated in miniature.
    assert_eq!(other.get(id), None);
    // Once it has a value in that slot it WOULD answer — hence "must not
    // outlive its query".
    let same_slot = other.intern("something else").unwrap();
    assert_eq!(same_slot, id);
    assert_eq!(other.get(id), Some("something else"));

    // Not a computed id at all.
    assert_eq!(values.get(inline_integer(7).unwrap()), None);
    assert_eq!(values.get(1234), None);
}

// ---------------------------------------------------------------------------
// The invariant: a computed id must never be stored
// ---------------------------------------------------------------------------

#[test]
fn in_memory_insert_rejects_a_computed_id_in_any_position() {
    let mut dict = TermDictionary::new();
    let mut values = QueryValues::new();
    let computed = values.intern("Entity").unwrap();
    let a = dict.intern("http://example.org/a");
    let p = dict.intern("http://example.org/p");

    for triple in [
        Triple::new(computed, p, a),
        Triple::new(a, computed, a),
        Triple::new(a, p, computed),
    ] {
        let mut store = TripleStore::new();
        match store.insert(triple) {
            Err(CoreError::ComputedValueNotStorable(id)) => assert_eq!(id, computed),
            other => panic!("expected rejection, got {:?}", other.map(|_| "Ok")),
        }
        assert_eq!(store.len(), 0);
    }
}

#[test]
fn ordinary_triples_still_insert() {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let a = dict.intern("http://example.org/a");
    let p = dict.intern("http://example.org/p");
    store
        .insert(Triple::new(a, p, inline_integer(42).unwrap()))
        .unwrap();
    store
        .insert(Triple::new(a, p, inline_boolean(true)))
        .unwrap();
    assert_eq!(store.len(), 2);
}

#[test]
fn persistent_insert_rejects_a_computed_id() {
    let dir = tempfile::tempdir().unwrap();
    let store = PersistentStore::open(dir.path()).unwrap();
    let mut dict = TermDictionary::new();
    let mut values = QueryValues::new();
    let computed = values.intern("Entity").unwrap();
    let a = dict.intern("http://example.org/a");
    let p = dict.intern("http://example.org/p");

    match store.insert(Triple::new(a, p, computed)) {
        Err(CoreError::ComputedValueNotStorable(id)) => assert_eq!(id, computed),
        other => panic!("expected rejection, got {:?}", other.map(|_| "Ok")),
    }
}

#[test]
fn persistent_batch_insert_rejects_a_computed_id_without_writing_the_rest() {
    use loka_core::BatchInsert;

    let dir = tempfile::tempdir().unwrap();
    let store = PersistentStore::open(dir.path()).unwrap();
    let mut dict = TermDictionary::new();
    let mut values = QueryValues::new();
    let computed = values.intern("Entity").unwrap();
    let a = dict.intern("http://example.org/a");
    let p = dict.intern("http://example.org/p");
    let o = dict.intern("http://example.org/o");

    let items = vec![
        BatchInsert {
            triple: Triple::new(a, p, o),
            subject: "http://example.org/a".into(),
            predicate: "http://example.org/p".into(),
            object: "http://example.org/o".into(),
            quoted: None,
        },
        BatchInsert {
            triple: Triple::new(a, p, computed),
            subject: "http://example.org/a".into(),
            predicate: "http://example.org/p".into(),
            object: "computed".into(),
            quoted: None,
        },
    ];

    assert!(matches!(
        store.insert_batch(&items),
        Err(CoreError::ComputedValueNotStorable(_))
    ));
    // The check runs before the transaction opens, so the good row in the same
    // batch is not half-committed either.
    assert_eq!(store.len(), 0);
}
