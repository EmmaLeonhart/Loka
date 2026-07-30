//! IRI interning and RDF-star ID scheme.
//!
//! All IRIs, blank nodes, and literals are interned to `u64` IDs at write time.
//! Quoted triples (RDF-star) are content-addressed via xxHash3 of their (S, P, O) tuple.
//!
//! # Inline literals (inspired by Jena TDB2)
//!
//! Small typed literals (integers, booleans) can be encoded directly into
//! the 64-bit TermId without a dictionary lookup. The high bit distinguishes
//! inline values from dictionary pointers:
//!
//! - Bit 63 = 0: dictionary pointer (regular term)
//! - Bit 63 = 1: inline value
//! - Bits 62-56: type tag (7 bits, 128 possible inline types)
//! - Bits 55-0: payload (56 bits)
//!
//! This avoids dictionary lookups for numeric filters and comparisons,
//! which is a major performance win for SPARQL queries over typed data.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use xxhash_rust::xxh3::xxh3_64;

/// A 64-bit interned identifier for any RDF term.
pub type TermId = u64;

/// Sentinel value meaning "no such term."
pub const INVALID_ID: TermId = 0;

// --- Inline literal encoding ---

const INLINE_BIT: u64 = 1 << 63;
const TYPE_TAG_SHIFT: u32 = 56;
const TYPE_TAG_MASK: u64 = 0x7F << TYPE_TAG_SHIFT;
const PAYLOAD_MASK: u64 = (1 << 56) - 1;

/// Type tags for inline literals.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineType {
    Integer = 0x01,
    Boolean = 0x02,
    /// Temporal literal: 48-bit timestamp + 4-bit precision.
    Temporal = 0x03,
    // Future: Float = 0x04, etc.
    /// A value **computed during one query**, not a term in the graph: the
    /// payload is an index into that query's [`QueryValues`] table, not a value.
    ///
    /// Deliberately the top of the tag space rather than the next free number,
    /// so it reads as "not a normal literal type" and leaves 0x04.. free.
    ///
    /// **A `Computed` id must never be stored.** Its payload only means anything
    /// while the query that produced it is alive; persisted, it would later
    /// resolve to whatever value happened to land in that slot — silent data
    /// corruption rather than a wrong answer. `TripleStore::insert` and the
    /// persistent insert paths reject it. See `planning/computed-values.md`.
    Computed = 0x7F,
}

/// Check if a TermId is an inline literal (not a dictionary pointer).
pub fn is_inline(id: TermId) -> bool {
    id & INLINE_BIT != 0
}

/// Encode a signed integer as an inline TermId.
/// The integer must fit in 56 bits signed (roughly ±36 quadrillion).
/// Returns `None` if out of range.
pub fn inline_integer(value: i64) -> Option<TermId> {
    // 56-bit signed range: -(2^55) to (2^55 - 1)
    if !(-(1i64 << 55)..(1i64 << 55)).contains(&value) {
        return None;
    }
    let payload = (value as u64) & PAYLOAD_MASK;
    Some(INLINE_BIT | ((InlineType::Integer as u64) << TYPE_TAG_SHIFT) | payload)
}

/// Decode an inline integer TermId back to i64.
/// Returns `None` if the TermId is not an inline integer.
pub fn decode_inline_integer(id: TermId) -> Option<i64> {
    if !is_inline(id) {
        return None;
    }
    let tag = (id & TYPE_TAG_MASK) >> TYPE_TAG_SHIFT;
    if tag != InlineType::Integer as u64 {
        return None;
    }
    let payload = id & PAYLOAD_MASK;
    // Sign-extend from 56 bits to 64 bits
    let value = if payload & (1 << 55) != 0 {
        (payload | !PAYLOAD_MASK) as i64
    } else {
        payload as i64
    };
    Some(value)
}

/// Encode a boolean as an inline TermId.
pub fn inline_boolean(value: bool) -> TermId {
    INLINE_BIT | ((InlineType::Boolean as u64) << TYPE_TAG_SHIFT) | (value as u64)
}

/// Decode an inline boolean TermId.
pub fn decode_inline_boolean(id: TermId) -> Option<bool> {
    if !is_inline(id) {
        return None;
    }
    let tag = (id & TYPE_TAG_MASK) >> TYPE_TAG_SHIFT;
    if tag != InlineType::Boolean as u64 {
        return None;
    }
    Some((id & PAYLOAD_MASK) != 0)
}

/// Get the inline type tag, if this is an inline value.
pub fn inline_type(id: TermId) -> Option<InlineType> {
    if !is_inline(id) {
        return None;
    }
    let tag = ((id & TYPE_TAG_MASK) >> TYPE_TAG_SHIFT) as u8;
    match tag {
        0x01 => Some(InlineType::Integer),
        0x02 => Some(InlineType::Boolean),
        0x03 => Some(InlineType::Temporal),
        0x7F => Some(InlineType::Computed),
        _ => None,
    }
}

/// True if this id is a per-query computed value (see [`InlineType::Computed`]).
///
/// The storage boundary checks this; nothing may persist one.
pub fn is_computed(id: TermId) -> bool {
    inline_type(id) == Some(InlineType::Computed)
}

/// Values computed while evaluating ONE query — the results of `BIND`,
/// projected expressions and the like, which are not terms in the graph and so
/// have no dictionary id.
///
/// Why not just intern them: the dictionary is persisted and `execute` holds it
/// immutably, so interning query-derived strings would grow the stored
/// dictionary from read-only traffic and turn every query into a writer. Full
/// reasoning, alternatives and staging: `planning/computed-values.md`.
///
/// Values are interned **within the query**, so two rows that compute the same
/// string get the same id and `DISTINCT` / `GROUP BY` / joins on a computed
/// variable behave. Ordering must still compare the *strings* — an id here
/// reflects the order values were first computed, exactly the trap that made
/// negative-integer ordering wrong before 2026-07-29.
#[derive(Debug, Default, Clone)]
pub struct QueryValues {
    values: Vec<String>,
    by_value: HashMap<String, TermId>,
}

impl QueryValues {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a computed value and return its id, or `None` if the table is
    /// full (2^56 distinct values in one query — a bound this cannot reach in
    /// practice, returned rather than panicked because the caller already knows
    /// how to treat a value it cannot represent: leave the variable unbound).
    pub fn intern(&mut self, value: &str) -> Option<TermId> {
        if let Some(&id) = self.by_value.get(value) {
            return Some(id);
        }
        let index = self.values.len() as u64;
        if index > PAYLOAD_MASK {
            return None;
        }
        let id = INLINE_BIT | ((InlineType::Computed as u64) << TYPE_TAG_SHIFT) | index;
        self.values.push(value.to_string());
        self.by_value.insert(value.to_string(), id);
        Some(id)
    }

    /// Resolve a computed id back to its value. `None` for any other id,
    /// including one minted by a *different* `QueryValues` whose table was
    /// shorter — which is why these ids must not outlive their query.
    pub fn get(&self, id: TermId) -> Option<&str> {
        if !is_computed(id) {
            return None;
        }
        self.values
            .get((id & PAYLOAD_MASK) as usize)
            .map(|s| s.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }
}

// --- Term Dictionary ---

/// Bidirectional dictionary that maps string terms to integer IDs and back.
///
/// Thread safety: designed for single-writer usage. For concurrent access,
/// wrap in an `RwLock` at the store level.
pub struct TermDictionary {
    forward: HashMap<String, TermId>,
    reverse: HashMap<TermId, String>,
    /// Reverse map for RDF-star quoted triples. `quoted_triple_id(s,p,o)` is a
    /// content hash (one-way), so without this map a quoted-triple TermId
    /// cannot be rendered back to `<< s p o >>` and provenance cascades can't
    /// dereference a `propositionInferredFrom` source id. Populated whenever a
    /// quoted triple is registered; persisted via the `quoted` sled tree and
    /// rehydrated on startup.
    quoted: HashMap<TermId, [TermId; 3]>,
    next_id: AtomicU64,
}

impl TermDictionary {
    /// Create an empty dictionary. IDs start at 1 (0 is reserved as invalid).
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
            quoted: HashMap::new(),
            next_id: AtomicU64::new(1),
        }
    }

    /// Intern a string term, returning its ID. If already interned, returns the existing ID.
    pub fn intern(&mut self, term: &str) -> TermId {
        if let Some(&id) = self.forward.get(term) {
            return id;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // Safety: dictionary IDs must never collide with inline IDs.
        // Since dictionary IDs start at 1 and grow, and inline IDs have
        // bit 63 set, there's no overlap in the practical range.
        debug_assert!(!is_inline(id), "dictionary ID space exhausted");
        self.forward.insert(term.to_owned(), id);
        self.reverse.insert(id, term.to_owned());
        id
    }

    /// Look up a term by its ID. Returns None for inline literal IDs.
    pub fn resolve(&self, id: TermId) -> Option<&str> {
        self.reverse.get(&id).map(|s| s.as_str())
    }

    /// Look up an ID by its string term.
    pub fn lookup(&self, term: &str) -> Option<TermId> {
        self.forward.get(term).copied()
    }

    /// Insert a term with a specific pre-assigned ID.
    /// Used when hydrating from a persistent store that already has assigned IDs.
    /// Updates next_id if the given id is >= current next_id.
    pub fn insert_with_id(&mut self, term: &str, id: TermId) {
        self.forward.insert(term.to_owned(), id);
        self.reverse.insert(id, term.to_owned());
        // Ensure next_id stays ahead of all loaded IDs
        let next = self.next_id.load(Ordering::Relaxed);
        if id >= next {
            self.next_id.store(id + 1, Ordering::Relaxed);
        }
    }

    /// Register an RDF-star quoted triple `<< s p o >>` and return its
    /// content-addressed TermId. Idempotent: re-registering the same (s,p,o)
    /// returns the same id and re-records the (now identical) reverse entry.
    pub fn register_quoted(&mut self, s: TermId, p: TermId, o: TermId) -> TermId {
        let id = quoted_triple_id(s, p, o);
        self.quoted.insert(id, [s, p, o]);
        id
    }

    /// Resolve a quoted-triple id back to its inner `(s, p, o)`.
    /// Returns `None` for non-quoted ids.
    pub fn resolve_quoted(&self, id: TermId) -> Option<(TermId, TermId, TermId)> {
        self.quoted.get(&id).map(|&[s, p, o]| (s, p, o))
    }

    /// Insert a quoted-triple mapping with a pre-computed id. Used when
    /// rehydrating the reverse map from a persistent store. The caller
    /// guarantees `id == quoted_triple_id(s, p, o)`.
    pub fn insert_quoted_with_id(&mut self, id: TermId, s: TermId, p: TermId, o: TermId) {
        self.quoted.insert(id, [s, p, o]);
    }

    /// Number of registered quoted triples.
    pub fn quoted_len(&self) -> usize {
        self.quoted.len()
    }

    /// Render a TermId to a display string, resolving quoted-triple ids
    /// recursively to N-Triples-star `<< s p o >>` form. This is a faithful
    /// drop-in for `resolve` for plain terms (same bare string) and the
    /// reason a quoted-triple subject no longer has to fall back to a
    /// `_:idN` blank node or a `<<QUOTED_TRIPLE>>` sentinel.
    /// Returns `None` only for ids that are neither inline, interned, nor
    /// a registered quoted triple.
    pub fn render_term(&self, id: TermId) -> Option<String> {
        if let Some(n) = decode_inline_integer(id) {
            return Some(n.to_string());
        }
        if let Some(b) = decode_inline_boolean(id) {
            return Some(b.to_string());
        }
        if let Some(t) = self.reverse.get(&id) {
            return Some(t.clone());
        }
        if self.quoted.contains_key(&id) {
            return Some(self.quoted_component_str(id, 0));
        }
        None
    }

    /// Format a single position of a quoted triple. IRIs are angle-bracketed
    /// so the result is parseable N-Triples-star; literals/blank nodes are
    /// emitted verbatim (they are stored with their lexical markers). Bounded
    /// recursion: nested quoting deeper than 8 is treated as opaque (content
    /// hashes make a real cycle astronomically unlikely, but crafted data
    /// must not be able to blow the stack).
    fn quoted_component_str(&self, id: TermId, depth: u8) -> String {
        if let Some(n) = decode_inline_integer(id) {
            return n.to_string();
        }
        if let Some(b) = decode_inline_boolean(id) {
            return b.to_string();
        }
        if depth < 8 {
            if let Some(&[s, p, o]) = self.quoted.get(&id) {
                return format!(
                    "<< {} {} {} >>",
                    self.quoted_component_str(s, depth + 1),
                    self.quoted_component_str(p, depth + 1),
                    self.quoted_component_str(o, depth + 1),
                );
            }
        }
        match self.reverse.get(&id) {
            Some(t) if t.starts_with('"') || t.starts_with("_:") => t.clone(),
            Some(t) => format!("<{}>", t),
            None => format!("_:id{}", id),
        }
    }

    /// Number of interned terms.
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// Whether the dictionary is empty.
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }
}

impl Default for TermDictionary {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a content-addressed ID for a quoted triple (RDF-star).
///
/// The ID is the xxHash3 of the concatenation of subject, predicate, and object IDs.
/// This gives us a deterministic u64 for any (S, P, O) tuple.
pub fn quoted_triple_id(subject: TermId, predicate: TermId, object: TermId) -> TermId {
    let mut buf = [0u8; 24];
    buf[0..8].copy_from_slice(&subject.to_le_bytes());
    buf[8..16].copy_from_slice(&predicate.to_le_bytes());
    buf[16..24].copy_from_slice(&object.to_le_bytes());
    let hash = xxh3_64(&buf);
    // Ensure we never return 0 (INVALID_ID) or an inline-flagged value
    let id = hash & !INLINE_BIT;
    if id == 0 {
        1
    } else {
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- TermDictionary ---

    #[test]
    fn intern_and_resolve() {
        let mut dict = TermDictionary::new();
        let id1 = dict.intern("http://example.org/Alice");
        let id2 = dict.intern("http://example.org/Bob");
        let id1_again = dict.intern("http://example.org/Alice");

        assert_eq!(id1, id1_again);
        assert_ne!(id1, id2);
        assert_eq!(dict.resolve(id1), Some("http://example.org/Alice"));
        assert_eq!(dict.resolve(id2), Some("http://example.org/Bob"));
        assert_eq!(dict.len(), 2);
    }

    #[test]
    fn lookup_missing() {
        let dict = TermDictionary::new();
        assert_eq!(dict.lookup("nonexistent"), None);
        assert_eq!(dict.resolve(999), None);
    }

    #[test]
    fn dictionary_ids_not_inline() {
        let mut dict = TermDictionary::new();
        for i in 0..100 {
            let id = dict.intern(&format!("http://example.org/{}", i));
            assert!(
                !is_inline(id),
                "dictionary ID should not have inline bit set"
            );
        }
    }

    // --- Quoted triple IDs ---

    #[test]
    fn quoted_triple_id_deterministic() {
        let id_a = quoted_triple_id(1, 2, 3);
        let id_b = quoted_triple_id(1, 2, 3);
        let id_c = quoted_triple_id(3, 2, 1);

        assert_eq!(id_a, id_b);
        assert_ne!(id_a, id_c);
        assert_ne!(id_a, INVALID_ID);
        assert!(!is_inline(id_a), "quoted triple ID should not be inline");
    }

    #[test]
    fn register_and_resolve_quoted() {
        let mut dict = TermDictionary::new();
        let s = dict.intern("http://example.org/Q42");
        let p = dict.intern("http://example.org/P20");
        let o = dict.intern("\"Belgium\"");

        let qid = dict.register_quoted(s, p, o);
        assert_eq!(qid, quoted_triple_id(s, p, o), "id is the content hash");
        assert_eq!(dict.resolve_quoted(qid), Some((s, p, o)));
        // Idempotent.
        assert_eq!(dict.register_quoted(s, p, o), qid);
        assert_eq!(dict.quoted_len(), 1);
        // A non-quoted id resolves to None.
        assert_eq!(dict.resolve_quoted(s), None);
    }

    #[test]
    fn render_term_quoted_is_faithful_ntriples_star() {
        let mut dict = TermDictionary::new();
        let s = dict.intern("http://example.org/Q42");
        let p = dict.intern("http://example.org/P20");
        let o = dict.intern("\"Belgium\"");
        let qid = dict.register_quoted(s, p, o);

        // Plain terms render exactly as stored (drop-in for resolve).
        assert_eq!(
            dict.render_term(s).as_deref(),
            Some("http://example.org/Q42")
        );
        // The quoted id renders faithfully, NOT as _:idN or a sentinel.
        assert_eq!(
            dict.render_term(qid).as_deref(),
            Some("<< <http://example.org/Q42> <http://example.org/P20> \"Belgium\" >>")
        );
        // Unknown id → None.
        assert_eq!(dict.render_term(999_999), None);
    }

    #[test]
    fn render_term_nested_quoted() {
        // << << s p o >> qp qo >>  — RDF-star annotation on a quoted triple.
        let mut dict = TermDictionary::new();
        let s = dict.intern("http://example.org/s");
        let p = dict.intern("http://example.org/p");
        let o = dict.intern("http://example.org/o");
        let inner = dict.register_quoted(s, p, o);
        let qp = dict.intern("http://loka.dev/provenance/propositionConfidence");
        let qo = dict.intern("\"0.9\"");
        let outer = dict.register_quoted(inner, qp, qo);

        assert_eq!(
            dict.render_term(outer).as_deref(),
            Some(
                "<< << <http://example.org/s> <http://example.org/p> <http://example.org/o> >> \
                 <http://loka.dev/provenance/propositionConfidence> \"0.9\" >>"
            )
        );
    }

    #[test]
    fn insert_quoted_with_id_rehydrates() {
        // Simulate persistence rehydration: a fresh dict gets the mapping
        // injected with a pre-computed id (as load_quoted_into would do).
        let mut src = TermDictionary::new();
        let s = src.intern("http://example.org/a");
        let p = src.intern("http://example.org/b");
        let o = src.intern("http://example.org/c");
        let qid = src.register_quoted(s, p, o);

        let mut fresh = TermDictionary::new();
        fresh.intern("http://example.org/a");
        fresh.intern("http://example.org/b");
        fresh.intern("http://example.org/c");
        fresh.insert_quoted_with_id(qid, s, p, o);
        assert_eq!(fresh.resolve_quoted(qid), Some((s, p, o)));
        assert_eq!(fresh.render_term(qid), src.render_term(qid));
    }

    #[test]
    fn quoted_triple_id_not_zero() {
        // Hash any reasonable input; result should never be 0
        for i in 0..100u64 {
            let id = quoted_triple_id(i, i + 1, i + 2);
            assert_ne!(id, INVALID_ID);
        }
    }

    // --- Inline integers ---

    #[test]
    fn inline_integer_roundtrip() {
        for &val in &[0i64, 1, -1, 42, -42, 1000000, -1000000] {
            let id = inline_integer(val).unwrap();
            assert!(is_inline(id));
            assert_eq!(inline_type(id), Some(InlineType::Integer));
            assert_eq!(decode_inline_integer(id), Some(val));
        }
    }

    #[test]
    fn inline_integer_max_range() {
        let max = (1i64 << 55) - 1;
        let min = -(1i64 << 55);

        assert!(inline_integer(max).is_some());
        assert_eq!(
            decode_inline_integer(inline_integer(max).unwrap()),
            Some(max)
        );

        assert!(inline_integer(min).is_some());
        assert_eq!(
            decode_inline_integer(inline_integer(min).unwrap()),
            Some(min)
        );

        // Out of range
        assert!(inline_integer(max + 1).is_none());
        assert!(inline_integer(min - 1).is_none());
    }

    #[test]
    fn inline_integer_not_confused_with_dict() {
        let mut dict = TermDictionary::new();
        let dict_id = dict.intern("http://example.org/x");
        let inline_id = inline_integer(42).unwrap();

        assert!(!is_inline(dict_id));
        assert!(is_inline(inline_id));
        assert_ne!(dict_id, inline_id);
    }

    // --- Inline booleans ---

    #[test]
    fn inline_boolean_roundtrip() {
        let true_id = inline_boolean(true);
        let false_id = inline_boolean(false);

        assert!(is_inline(true_id));
        assert!(is_inline(false_id));
        assert_ne!(true_id, false_id);

        assert_eq!(decode_inline_boolean(true_id), Some(true));
        assert_eq!(decode_inline_boolean(false_id), Some(false));

        assert_eq!(inline_type(true_id), Some(InlineType::Boolean));
    }

    #[test]
    fn decode_wrong_type() {
        let int_id = inline_integer(42).unwrap();
        let bool_id = inline_boolean(true);

        assert_eq!(decode_inline_boolean(int_id), None);
        assert_eq!(decode_inline_integer(bool_id), None);
    }

    #[test]
    fn decode_non_inline() {
        assert_eq!(decode_inline_integer(42), None);
        assert_eq!(decode_inline_boolean(42), None);
        assert_eq!(inline_type(42), None);
    }
}
