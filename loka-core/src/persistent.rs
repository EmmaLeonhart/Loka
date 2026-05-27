//! Persistent triple store backed by sled.
//!
//! Uses sled's sorted key-value trees to implement SPO/POS/OSP indexes
//! that survive restarts. Same API as the in-memory `TripleStore`, but
//! data is durably written to disk.
//!
//! Each index is a separate sled `Tree` within a single `Db`. Keys are
//! 24-byte composite keys (3 x u64 in big-endian for correct sort order).
//! Values are empty — the key itself encodes the triple.

use std::path::Path;

use sled::Transactional;

use crate::error::{CoreError, Result};
use crate::id::{quoted_triple_id, TermId};
use crate::temporal::{decode_tspo_key, tspo_key, TemporalSignifier};
use crate::triple::Triple;

/// One row for `PersistentStore::insert_batch`. The caller provides the
/// pre-computed `Triple` (so SPO/POS/OSP keys can be derived) plus the
/// string forms of subject / predicate / object so we can intern them
/// inside the same sled transaction.
#[derive(Debug, Clone)]
pub struct BatchInsert {
    pub triple: Triple,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    /// If this row involves an RDF-star quoted triple, the inner
    /// `(s, p, o)` term ids to register in the reverse map so the
    /// content-addressed `quoted_triple_id` can be reversed (faithful
    /// rendering + provenance cascade). `None` for ordinary rows.
    pub quoted: Option<(TermId, TermId, TermId)>,
}

/// A persistent triple store backed by sled.
///
/// Three sled trees provide SPO/POS/OSP indexes with the same semantics
/// as the in-memory `TripleStore`. A fourth tree stores the term dictionary.
pub struct PersistentStore {
    #[allow(dead_code)]
    db: sled::Db,
    spo: sled::Tree,
    pos: sled::Tree,
    osp: sled::Tree,
    /// Time → Subject → Predicate → Object index (ontochronological).
    /// 33-byte keys: [signifier:1 | timestamp:8 | S:8 | P:8 | O:8].
    tspo: sled::Tree,
    /// Term dictionary: forward map (string → u64 ID).
    terms_forward: sled::Tree,
    /// Term dictionary: reverse map (u64 ID → string).
    terms_reverse: sled::Tree,
    /// RDF-star quoted-triple reverse map: quoted_triple_id (8-byte LE key)
    /// → inner (s, p, o) ids (24-byte LE value). Lets a content-addressed
    /// quoted-triple id be reversed to its components — required for
    /// faithful rendering and provenance cascade-retraction.
    quoted: sled::Tree,
    /// Next term ID counter, stored persistently.
    meta: sled::Tree,
}

const NEXT_ID_KEY: &[u8] = b"next_term_id";

impl PersistentStore {
    /// Open or create a persistent store at the given path.
    ///
    /// Sled is configured explicitly (not via `sled::open`) so we can
    /// constrain its memory + fsync footprint. Defaults (1 GB cache,
    /// 500 ms fsync interval, `LowSpaceUsage` mode) panic the periodic
    /// flusher with Win32 `ERROR_NO_SYSTEM_RESOURCES` (os error 1450)
    /// at ~33 M triples / 5 GB on Windows — observed on the 2026-05-12
    /// big-corpus ingest. The combination is too much in-flight write
    /// activity for the OS I/O manager.
    ///
    /// The tuning here cuts both ends of the pressure:
    /// - `cache_capacity(256 MB)` — quarter of the default. The hot SPO
    ///   prefix is small; we don't need a gigabyte cached.
    /// - `flush_every_ms(2000)` — fsync every 2s instead of every 500ms.
    ///   Quarters the fsync rate. Durability window grows from 0.5s to
    ///   2s, which is fine for our workload (bulk ingest is replayable
    ///   from the HF import state file).
    /// - `mode(HighThroughput)` — sled's "batch more writes before
    ///   committing" mode. Trades some space-amplification for far less
    ///   per-fsync work.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::Config::new()
            .path(path.as_ref())
            .cache_capacity(256 * 1024 * 1024)
            .flush_every_ms(Some(2000))
            .mode(sled::Mode::HighThroughput)
            .open()?;
        let spo = db.open_tree("spo")?;
        let pos = db.open_tree("pos")?;
        let osp = db.open_tree("osp")?;
        let tspo = db.open_tree("tspo")?;
        let terms_forward = db.open_tree("terms_fwd")?;
        let terms_reverse = db.open_tree("terms_rev")?;
        let quoted = db.open_tree("quoted")?;
        let meta = db.open_tree("meta")?;

        // Initialize next_id if not present (start at 1, 0 is INVALID_ID)
        if meta.get(NEXT_ID_KEY)?.is_none() {
            meta.insert(NEXT_ID_KEY, &1u64.to_le_bytes())?;
        }

        Ok(Self {
            db,
            spo,
            pos,
            osp,
            tspo,
            terms_forward,
            terms_reverse,
            quoted,
            meta,
        })
    }

    /// Open a temporary store (for testing). Data is deleted on drop.
    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        let spo = db.open_tree("spo")?;
        let pos = db.open_tree("pos")?;
        let osp = db.open_tree("osp")?;
        let tspo = db.open_tree("tspo")?;
        let terms_forward = db.open_tree("terms_fwd")?;
        let terms_reverse = db.open_tree("terms_rev")?;
        let quoted = db.open_tree("quoted")?;
        let meta = db.open_tree("meta")?;

        meta.insert(NEXT_ID_KEY, &1u64.to_le_bytes())?;

        Ok(Self {
            db,
            spo,
            pos,
            osp,
            tspo,
            terms_forward,
            terms_reverse,
            quoted,
            meta,
        })
    }

    // --- Triple operations ---

    /// Insert a triple atomically across all three indexes.
    /// Returns `Err(DuplicateTriple)` if already present.
    /// Uses sled's multi-tree transaction to ensure SPO/POS/OSP are
    /// always consistent — either all three are written or none are.
    pub fn insert(&self, triple: Triple) -> Result<()> {
        let spo_key = triple.spo_key();
        let pos_key = triple.pos_key();
        let osp_key = triple.osp_key();

        (&self.spo, &self.pos, &self.osp)
            .transaction(|(spo, pos, osp)| {
                if spo.get(spo_key)?.is_some() {
                    return sled::transaction::abort(());
                }
                spo.insert(spo_key.as_ref(), &[] as &[u8])?;
                pos.insert(pos_key.as_ref(), &[] as &[u8])?;
                osp.insert(osp_key.as_ref(), &[] as &[u8])?;
                Ok(())
            })
            .map_err(|e| match e {
                sled::transaction::TransactionError::Abort(()) => CoreError::DuplicateTriple,
                sled::transaction::TransactionError::Storage(e) => CoreError::Sled(e),
            })
    }

    /// Insert a batch of triples + their string terms in a single sled
    /// transaction across all 5 trees (spo, pos, osp, terms_forward,
    /// terms_reverse).
    ///
    /// Why this exists: the naive `insert` + `intern` loop calls sled
    /// transactions per triple (≥ 4 transactions per N-Triple line:
    /// 3 interns + 1 insert). Under sustained ingest of ~100k+ triples in
    /// one POST request, sled accumulates uncommitted write-ahead-log work
    /// faster than its internal compactor can drain, and the writer thread
    /// eventually wedges (`POST /triples` times out, `/health` keeps
    /// responding) — the engine-bug-at-scale documented in paper §6.1.
    ///
    /// One transaction per batch (rather than per triple) reduces sled
    /// internal book-keeping by 3–4 orders of magnitude on bulk ingest and
    /// keeps the WAL bounded.
    ///
    /// Each `BatchInsert` is one row to ingest. The caller is responsible
    /// for in-memory ID assignment; we just need to know the string forms
    /// of subject/predicate/object plus the corresponding pre-computed
    /// `Triple` so we can write the SPO/POS/OSP keys atomically.
    ///
    /// Returns the number of triples that were *newly* inserted (duplicates
    /// silently skipped — they're not errors at batch scale).
    pub fn insert_batch(&self, items: &[BatchInsert]) -> Result<usize> {
        // sled transactions take `Fn` (not `FnMut`) closures and may be
        // re-invoked on optimistic-concurrency conflict, so we can't mutate
        // captured variables. The counter and "inserted" count are local
        // to each closure invocation; we read the persisted counter from
        // `meta` at the start of each attempt and return the new count.
        let inserted = (
            &self.spo,
            &self.pos,
            &self.osp,
            &self.terms_forward,
            &self.terms_reverse,
            &self.quoted,
            &self.meta,
        )
            .transaction(
                |(spo, pos, osp, terms_fwd, terms_rev, quoted, meta)| -> sled::transaction::ConflictableTransactionResult<usize, ()> {
                    let mut next_id_counter = match meta.get(NEXT_ID_KEY)? {
                        Some(b) if b.len() == 8 => {
                            let mut arr = [0u8; 8];
                            arr.copy_from_slice(&b);
                            u64::from_le_bytes(arr)
                        }
                        _ => 1u64,
                    };
                    let mut inserted_this_attempt = 0usize;

                    for item in items {
                        for term in [
                            item.subject.as_str(),
                            item.predicate.as_str(),
                            item.object.as_str(),
                        ] {
                            if terms_fwd.get(term.as_bytes())?.is_none() {
                                let id_bytes = next_id_counter.to_le_bytes();
                                terms_fwd.insert(term.as_bytes(), &id_bytes)?;
                                terms_rev.insert(&id_bytes, term.as_bytes())?;
                                next_id_counter += 1;
                            }
                        }

                        let spo_key = item.triple.spo_key();
                        if spo.get(spo_key)?.is_none() {
                            spo.insert(spo_key.as_ref(), &[] as &[u8])?;
                            pos.insert(item.triple.pos_key().as_ref(), &[] as &[u8])?;
                            osp.insert(item.triple.osp_key().as_ref(), &[] as &[u8])?;
                            inserted_this_attempt += 1;
                        }

                        // RDF-star quoted-triple reverse map. Written in the
                        // same transaction as the triple so the mapping can
                        // never be lost relative to the rows that need it.
                        if let Some((qs, qp, qo)) = item.quoted {
                            let qid = quoted_triple_id(qs, qp, qo);
                            if quoted.get(qid.to_le_bytes())?.is_none() {
                                quoted.insert(
                                    &qid.to_le_bytes(),
                                    &encode_spo(qs, qp, qo),
                                )?;
                            }
                        }
                    }

                    meta.insert(NEXT_ID_KEY, &next_id_counter.to_le_bytes())?;
                    Ok(inserted_this_attempt)
                },
            )
            .map_err(|e| match e {
                sled::transaction::TransactionError::Abort(()) => CoreError::DuplicateTriple,
                sled::transaction::TransactionError::Storage(e) => CoreError::Sled(e),
            })?;

        Ok(inserted)
    }

    /// Remove a triple atomically across all three indexes.
    /// Returns true if it was present.
    pub fn remove(&self, triple: &Triple) -> Result<bool> {
        let spo_key = triple.spo_key();
        let pos_key = triple.pos_key();
        let osp_key = triple.osp_key();

        let was_present: std::result::Result<bool, sled::transaction::TransactionError<()>> =
            (&self.spo, &self.pos, &self.osp).transaction(|(spo, pos, osp)| {
                let existed = spo.remove(spo_key.as_ref())?.is_some();
                if existed {
                    pos.remove(pos_key.as_ref())?;
                    osp.remove(osp_key.as_ref())?;
                }
                Ok(existed)
            });
        let was_present = was_present.map_err(|e| match e {
            sled::transaction::TransactionError::Abort(()) => {
                CoreError::Storage(std::io::Error::other("transaction aborted"))
            }
            sled::transaction::TransactionError::Storage(e) => CoreError::Sled(e),
        })?;

        Ok(was_present)
    }

    /// Check whether a triple exists.
    pub fn contains(&self, triple: &Triple) -> Result<bool> {
        Ok(self.spo.contains_key(triple.spo_key())?)
    }

    /// Number of triples in the store.
    pub fn len(&self) -> usize {
        self.spo.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.spo.is_empty()
    }

    /// Find all triples with the given subject.
    pub fn find_by_subject(&self, subject: TermId) -> Vec<Triple> {
        let (lo, hi) = prefix_range_24(0, subject);
        self.spo
            .range(lo..=hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| Triple::from_spo_key(&key_to_array(&k)))
            .collect()
    }

    /// Find all triples with the given predicate.
    pub fn find_by_predicate(&self, predicate: TermId) -> Vec<Triple> {
        let (lo, hi) = prefix_range_24(0, predicate);
        self.pos
            .range(lo..=hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| Triple::from_pos_key(&key_to_array(&k)))
            .collect()
    }

    /// Find all triples with the given object.
    pub fn find_by_object(&self, object: TermId) -> Vec<Triple> {
        let (lo, hi) = prefix_range_24(0, object);
        self.osp
            .range(lo..=hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| Triple::from_osp_key(&key_to_array(&k)))
            .collect()
    }

    /// Find all triples with the given subject and predicate.
    pub fn find_by_subject_predicate(&self, subject: TermId, predicate: TermId) -> Vec<Triple> {
        let (lo, hi) = prefix_range_24_2(subject, predicate);
        self.spo
            .range(lo..=hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| Triple::from_spo_key(&key_to_array(&k)))
            .collect()
    }

    /// Iterate all triples in SPO order.
    pub fn iter(&self) -> impl Iterator<Item = Triple> + '_ {
        self.spo
            .iter()
            .filter_map(|r| r.ok())
            .map(|(k, _)| Triple::from_spo_key(&key_to_array(&k)))
    }

    // --- Temporal (TSPO) index operations ---

    /// Insert a temporal index entry.
    pub fn insert_temporal(
        &self,
        signifier: TemporalSignifier,
        timestamp: i64,
        subject: TermId,
        predicate: TermId,
        object: TermId,
    ) -> Result<bool> {
        let key = tspo_key(signifier, timestamp, subject, predicate, object);
        let was_new = self.tspo.insert(key, &[] as &[u8])?.is_none();
        Ok(was_new)
    }

    /// Remove a temporal index entry. Returns true if it was present.
    pub fn remove_temporal(
        &self,
        signifier: TemporalSignifier,
        timestamp: i64,
        subject: TermId,
        predicate: TermId,
        object: TermId,
    ) -> Result<bool> {
        let key = tspo_key(signifier, timestamp, subject, predicate, object);
        Ok(self.tspo.remove(key)?.is_some())
    }

    /// Find all temporal entries for a given signifier within `[start, end)`.
    pub fn find_temporal_range(
        &self,
        signifier: TemporalSignifier,
        start: i64,
        end: i64,
    ) -> Vec<(i64, Triple)> {
        let lo = tspo_key(signifier, start, 0, 0, 0);
        let hi = tspo_key(signifier, end, u64::MAX, u64::MAX, u64::MAX);
        self.tspo
            .range(lo..hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| {
                let arr = key_to_array_33(&k);
                let (_, ts, s, p, o) = decode_tspo_key(&arr);
                (ts, Triple::new(s, p, o))
            })
            .collect()
    }

    /// Find all `ValidFrom` entries with timestamp ≤ `at`.
    pub fn find_valid_from_before(&self, at: i64) -> Vec<(i64, Triple)> {
        let lo = tspo_key(TemporalSignifier::ValidFrom, i64::MIN, 0, 0, 0);
        let hi = tspo_key(
            TemporalSignifier::ValidFrom,
            at,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        );
        self.tspo
            .range(lo..=hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| {
                let arr = key_to_array_33(&k);
                let (_, ts, s, p, o) = decode_tspo_key(&arr);
                (ts, Triple::new(s, p, o))
            })
            .collect()
    }

    /// Find all `ValidTo` entries with timestamp ≤ `at`.
    pub fn find_valid_to_before(&self, at: i64) -> Vec<(i64, Triple)> {
        let lo = tspo_key(TemporalSignifier::ValidTo, i64::MIN, 0, 0, 0);
        let hi = tspo_key(TemporalSignifier::ValidTo, at, u64::MAX, u64::MAX, u64::MAX);
        self.tspo
            .range(lo..=hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| {
                let arr = key_to_array_33(&k);
                let (_, ts, s, p, o) = decode_tspo_key(&arr);
                (ts, Triple::new(s, p, o))
            })
            .collect()
    }

    /// Find all `AssertedAt` entries at a specific time.
    pub fn find_asserted_at(&self, at: i64) -> Vec<Triple> {
        let lo = tspo_key(TemporalSignifier::AssertedAt, at, 0, 0, 0);
        let hi = tspo_key(
            TemporalSignifier::AssertedAt,
            at,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        );
        self.tspo
            .range(lo..=hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| {
                let arr = key_to_array_33(&k);
                let (_, _, s, p, o) = decode_tspo_key(&arr);
                Triple::new(s, p, o)
            })
            .collect()
    }

    /// Number of entries in the TSPO index.
    pub fn temporal_len(&self) -> usize {
        self.tspo.len()
    }

    // --- Term dictionary operations ---

    /// Intern a string term, returning its ID. If already interned, returns existing ID.
    pub fn intern(&self, term: &str) -> Result<TermId> {
        if let Some(id_bytes) = self.terms_forward.get(term.as_bytes())? {
            return bytes_to_u64(id_bytes.as_ref());
        }

        // Atomically increment the counter
        let id = self.next_id()?;
        let id_bytes = id.to_le_bytes();
        self.terms_forward.insert(term.as_bytes(), &id_bytes)?;
        self.terms_reverse.insert(id_bytes, term.as_bytes())?;
        Ok(id)
    }

    /// Register an RDF-star quoted triple's reverse mapping
    /// (`quoted_triple_id(s,p,o) → (s,p,o)`). Idempotent. Used by the
    /// non-batch insert path (e.g. SPARQL `INSERT DATA`); the bulk
    /// `/triples` path registers via `BatchInsert::quoted` inside
    /// `insert_batch`'s transaction instead.
    pub fn register_quoted(&self, s: TermId, p: TermId, o: TermId) -> Result<TermId> {
        let id = quoted_triple_id(s, p, o);
        if self.quoted.get(id.to_le_bytes())?.is_none() {
            self.quoted
                .insert(id.to_le_bytes(), &encode_spo(s, p, o))?;
        }
        Ok(id)
    }

    /// Rehydrate the in-memory quoted reverse map from disk. Returns the
    /// number of quoted triples loaded.
    pub fn load_quoted_into(&self, dict: &mut crate::id::TermDictionary) -> usize {
        let mut count = 0;
        for (key_bytes, val_bytes) in self.quoted.iter().flatten() {
            let id = match bytes_to_u64(key_bytes.as_ref()) {
                Ok(id) => id,
                Err(_) => continue,
            };
            if let Some((s, p, o)) = decode_spo(val_bytes.as_ref()) {
                dict.insert_quoted_with_id(id, s, p, o);
                count += 1;
            }
        }
        count
    }

    /// Look up a term by its ID.
    pub fn resolve(&self, id: TermId) -> Result<Option<String>> {
        match self.terms_reverse.get(id.to_le_bytes())? {
            Some(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
            None => Ok(None),
        }
    }

    /// Look up an ID by its string term.
    pub fn lookup(&self, term: &str) -> Result<Option<TermId>> {
        match self.terms_forward.get(term.as_bytes())? {
            Some(id_bytes) => Ok(Some(bytes_to_u64(id_bytes.as_ref())?)),
            None => Ok(None),
        }
    }

    /// Load all terms from persistent storage into an in-memory TermDictionary.
    /// Returns the number of terms loaded.
    pub fn load_terms_into(&self, dict: &mut crate::id::TermDictionary) -> usize {
        let mut count = 0;
        for (key_bytes, val_bytes) in self.terms_forward.iter().flatten() {
            let term = String::from_utf8_lossy(&key_bytes).into_owned();
            let id = match bytes_to_u64(val_bytes.as_ref()) {
                Ok(id) => id,
                Err(_) => continue, // skip corrupt entries
            };
            dict.insert_with_id(&term, id);
            count += 1;
        }
        // Hydrate the quoted reverse map too, so every existing
        // `load_terms_into` call site gets RDF-star reversal for free.
        let _ = self.load_quoted_into(dict);
        count
    }

    /// Find all triples with the given predicate and object.
    /// Uses the POS index for efficient lookup.
    pub fn find_by_predicate_object(&self, predicate: TermId, object: TermId) -> Vec<Triple> {
        let (lo, hi) = prefix_range_24_2(predicate, object);
        self.pos
            .range(lo..=hi)
            .filter_map(|r| r.ok())
            .map(|(k, _)| Triple::from_pos_key(&key_to_array(&k)))
            .collect()
    }

    /// Clear all triples from the store (all three indexes).
    /// Does not clear the term dictionary.
    pub fn clear(&self) -> Result<()> {
        self.spo.clear()?;
        self.pos.clear()?;
        self.osp.clear()?;
        self.tspo.clear()?;
        Ok(())
    }

    /// Verify index consistency: check that SPO/POS/OSP have the same count.
    /// Returns true if consistent, false if a crash may have caused partial writes.
    pub fn verify_consistency(&self) -> bool {
        let spo_count = self.spo.len();
        let pos_count = self.pos.len();
        let osp_count = self.osp.len();
        spo_count == pos_count && pos_count == osp_count
    }

    /// Repair indexes by rebuilding POS and OSP from SPO (the primary index).
    /// Call this after verify_consistency() returns false.
    pub fn repair(&self) -> Result<usize> {
        // Clear secondary indexes
        self.pos.clear()?;
        self.osp.clear()?;

        // Rebuild from SPO
        let mut count = 0;
        for (key, _) in self.spo.iter().flatten() {
            let arr = key_to_array(&key);
            let triple = Triple::from_spo_key(&arr);
            self.pos.insert(triple.pos_key(), &[])?;
            self.osp.insert(triple.osp_key(), &[])?;
            count += 1;
        }
        Ok(count)
    }

    /// Flush all pending writes to disk.
    pub fn flush(&self) -> Result<()> {
        self.spo.flush()?;
        self.pos.flush()?;
        self.osp.flush()?;
        self.tspo.flush()?;
        self.terms_forward.flush()?;
        self.terms_reverse.flush()?;
        self.quoted.flush()?;
        self.meta.flush()?;
        Ok(())
    }

    fn next_id(&self) -> Result<u64> {
        let old = self
            .meta
            .fetch_and_update(NEXT_ID_KEY, |old| {
                let bytes: [u8; 8] = old?.try_into().ok()?;
                let current = u64::from_le_bytes(bytes);
                Some((current + 1).to_le_bytes().to_vec())
            })?
            .ok_or(CoreError::CorruptValue {
                expected: 8,
                actual: 0,
            })?;
        bytes_to_u64(old.as_ref())
    }
}

/// Encode a quoted triple's inner (s, p, o) ids as a 24-byte LE value.
fn encode_spo(s: TermId, p: TermId, o: TermId) -> [u8; 24] {
    let mut v = [0u8; 24];
    v[0..8].copy_from_slice(&s.to_le_bytes());
    v[8..16].copy_from_slice(&p.to_le_bytes());
    v[16..24].copy_from_slice(&o.to_le_bytes());
    v
}

/// Decode a 24-byte LE value into (s, p, o) ids. `None` on length mismatch.
fn decode_spo(bytes: &[u8]) -> Option<(TermId, TermId, TermId)> {
    if bytes.len() != 24 {
        return None;
    }
    let s = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    let p = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    let o = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
    Some((s, p, o))
}

/// Convert a byte slice to a u64 term ID, returning an error on length mismatch.
fn bytes_to_u64(bytes: &[u8]) -> Result<u64> {
    let arr: [u8; 8] = bytes.try_into().map_err(|_| CoreError::CorruptValue {
        expected: 8,
        actual: bytes.len(),
    })?;
    Ok(u64::from_le_bytes(arr))
}

/// Build a 24-byte prefix range for a single leading u64.
fn prefix_range_24(_offset: usize, value: TermId) -> ([u8; 24], [u8; 24]) {
    let mut lo = [0u8; 24];
    lo[0..8].copy_from_slice(&value.to_be_bytes());
    let mut hi = [0u8; 24];
    hi[0..8].copy_from_slice(&value.to_be_bytes());
    hi[8..24].fill(0xFF);
    (lo, hi)
}

/// Build a 24-byte prefix range for two leading u64s.
fn prefix_range_24_2(first: TermId, second: TermId) -> ([u8; 24], [u8; 24]) {
    let mut lo = [0u8; 24];
    lo[0..8].copy_from_slice(&first.to_be_bytes());
    lo[8..16].copy_from_slice(&second.to_be_bytes());
    let mut hi = [0u8; 24];
    hi[0..8].copy_from_slice(&first.to_be_bytes());
    hi[8..16].copy_from_slice(&second.to_be_bytes());
    hi[16..24].fill(0xFF);
    (lo, hi)
}

fn key_to_array(ivec: &sled::IVec) -> [u8; 24] {
    let mut arr = [0u8; 24];
    arr.copy_from_slice(ivec.as_ref());
    arr
}

fn key_to_array_33(ivec: &sled::IVec) -> [u8; 33] {
    let mut arr = [0u8; 33];
    arr.copy_from_slice(ivec.as_ref());
    arr
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> PersistentStore {
        let store = PersistentStore::temporary().unwrap();
        store.insert(Triple::new(1, 10, 2)).unwrap();
        store.insert(Triple::new(1, 10, 3)).unwrap();
        store.insert(Triple::new(2, 10, 1)).unwrap();
        store.insert(Triple::new(1, 11, 100)).unwrap();
        store
    }

    #[test]
    fn insert_and_count() {
        let store = make_store();
        assert_eq!(store.len(), 4);
    }

    #[test]
    fn duplicate_rejected() {
        let store = make_store();
        let result = store.insert(Triple::new(1, 10, 2));
        assert!(result.is_err());
        assert_eq!(store.len(), 4);
    }

    #[test]
    fn find_by_subject() {
        let store = make_store();
        let results = store.find_by_subject(1);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn find_by_predicate() {
        let store = make_store();
        let results = store.find_by_predicate(10);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn find_by_object() {
        let store = make_store();
        let results = store.find_by_object(1);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn find_by_subject_predicate() {
        let store = make_store();
        let results = store.find_by_subject_predicate(1, 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn remove() {
        let store = make_store();
        assert!(store.remove(&Triple::new(1, 10, 2)).unwrap());
        assert_eq!(store.len(), 3);
        assert!(!store.contains(&Triple::new(1, 10, 2)).unwrap());
        assert_eq!(store.find_by_subject(1).len(), 2);
    }

    #[test]
    fn term_dictionary_roundtrip() {
        let store = PersistentStore::temporary().unwrap();
        let id1 = store.intern("http://example.org/Alice").unwrap();
        let id2 = store.intern("http://example.org/Bob").unwrap();
        let id1_again = store.intern("http://example.org/Alice").unwrap();

        assert_eq!(id1, id1_again);
        assert_ne!(id1, id2);
        assert_eq!(
            store.resolve(id1).unwrap().as_deref(),
            Some("http://example.org/Alice")
        );
        assert_eq!(
            store.resolve(id2).unwrap().as_deref(),
            Some("http://example.org/Bob")
        );
    }

    #[test]
    fn iter_all() {
        let store = make_store();
        let all: Vec<_> = store.iter().collect();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn quoted_survives_store_reopen() {
        // The real durability proof temporary() can't give: write a quoted
        // triple to an on-disk store, drop it, reopen, and confirm the
        // reverse map rehydrates through load_terms_into.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reopen.sdb");

        let (qid, s, p, o);
        {
            let ps = PersistentStore::open(&path).unwrap();
            s = ps.intern("http://wd/Q42").unwrap();
            p = ps.intern("http://wd/P20").unwrap();
            o = ps.intern("\"Belgium\"").unwrap();
            // inner asserted triple + the content-addressed annotation row.
            ps.insert(Triple::new(s, p, o)).unwrap();
            qid = ps.register_quoted(s, p, o).unwrap();
            let conf = ps
                .intern("http://loka.dev/provenance/propositionConfidence")
                .unwrap();
            let v = ps.intern("\"0.9\"").unwrap();
            ps.insert(Triple::new(qid, conf, v)).unwrap();
            ps.flush().unwrap();
        } // store dropped — sled closed.

        // Reopen the same path.
        let ps = PersistentStore::open(&path).unwrap();
        let mut dict = crate::id::TermDictionary::new();
        ps.load_terms_into(&mut dict); // also hydrates the quoted map.

        assert_eq!(
            dict.resolve_quoted(qid),
            Some((s, p, o)),
            "quoted reverse map survived reopen"
        );
        assert_eq!(
            dict.render_term(qid).as_deref(),
            Some("<< <http://wd/Q42> <http://wd/P20> \"Belgium\" >>"),
            "faithful render survives reopen"
        );
        // The annotation triple is still on disk.
        let conf = dict
            .lookup("http://loka.dev/provenance/propositionConfidence")
            .unwrap();
        assert!(!ps.find_by_subject(qid).is_empty());
        assert!(ps.find_by_subject(qid).iter().any(|t| t.predicate == conf));
    }

    #[test]
    fn quoted_register_and_hydrate() {
        let store = PersistentStore::temporary().unwrap();
        // Non-batch path.
        let qid = store.register_quoted(11, 22, 33).unwrap();
        assert_eq!(qid, quoted_triple_id(11, 22, 33));
        // Idempotent.
        assert_eq!(store.register_quoted(11, 22, 33).unwrap(), qid);

        // Batch path carries the mapping inside insert_batch's transaction.
        let qid2 = quoted_triple_id(44, 55, 66);
        store
            .insert_batch(&[BatchInsert {
                triple: Triple::new(qid2, 77, 88),
                subject: "<< a b c >>".into(),
                predicate: "http://loka.dev/provenance/propositionConfidence".into(),
                object: "\"0.9\"".into(),
                quoted: Some((44, 55, 66)),
            }])
            .unwrap();

        // Rehydrate into a fresh dictionary.
        let mut dict = crate::id::TermDictionary::new();
        let n = store.load_quoted_into(&mut dict);
        assert_eq!(n, 2);
        assert_eq!(dict.resolve_quoted(qid), Some((11, 22, 33)));
        assert_eq!(dict.resolve_quoted(qid2), Some((44, 55, 66)));

        // load_terms_into also hydrates quoted (free for all call sites).
        let mut dict2 = crate::id::TermDictionary::new();
        store.load_terms_into(&mut dict2);
        assert_eq!(dict2.resolve_quoted(qid2), Some((44, 55, 66)));
    }
}
