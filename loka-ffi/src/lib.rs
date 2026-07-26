#![allow(clippy::not_unsafe_ptr_arg_deref)]
//! C-compatible FFI layer for Loka.
//!
//! This crate produces a shared library (`.dll`/`.so`/`.dylib`) that can be
//! loaded by any language with FFI support — primarily Dart (`dart:ffi`) for
//! Loka Studio, but also usable from Python (`ctypes`), C, C++, etc.
//!
//! # Architecture
//!
//! The FFI library is designed so that **one process can host everything**:
//! - The database engine (loka-core, loka-hnsw, loka-sparql)
//! - The MCP server (optional, started via `loka_mcp_start`)
//! - The GUI (Loka Studio loads this library via dart:ffi)
//!
//! All three share the same database handle. The MCP server runs on a
//! background thread when started. The GUI is entirely in Flutter — this
//! library just provides the database operations.
//!
//! # Memory Management
//!
//! - Opaque handles (`LokaDb`, `LokaResult`) are heap-allocated and must
//!   be freed by the corresponding `_free` function.
//! - Strings returned by `loka_resolve`, `loka_health_report`, etc. are
//!   owned C strings that must be freed with `loka_string_free`.
//! - All functions are thread-safe — the opaque handle wraps `Arc<Mutex<...>>`.
//!
//! # Error Handling
//!
//! Functions return null pointers or negative integers on error. Call
//! `loka_last_error()` to get the error message.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};

// ─── Thread-local last error ─────────────────────────────────────────────────

thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
}

fn set_error(msg: &str) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = CString::new(msg).ok();
    });
}

/// Get the last error message. Returns null if no error.
/// The returned string is valid until the next FFI call on this thread.
#[no_mangle]
pub extern "C" fn loka_last_error() -> *const c_char {
    LAST_ERROR.with(|e| {
        e.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

// ─── Opaque types ────────────────────────────────────────────────────────────

/// Opaque database handle. Thread-safe, shareable across FFI boundary.
struct DbInner {
    ps: loka_core::PersistentStore,
    store: loka_core::TripleStore,
    dict: loka_core::TermDictionary,
    vectors: loka_hnsw::VectorRegistry,
}

/// Opaque handle passed across the FFI boundary.
pub struct LokaDb {
    inner: Arc<Mutex<DbInner>>,
}

/// Opaque query result handle.
pub struct LokaResult {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

// ─── Database lifecycle ──────────────────────────────────────────────────────

/// Open (or create) a Loka database at the given path.
///
/// Returns an opaque handle, or null on error (call `loka_last_error()`).
#[no_mangle]
pub extern "C" fn loka_db_open(path: *const c_char) -> *mut LokaDb {
    let path_str = match unsafe_cstr_to_str(path) {
        Some(s) => s,
        None => {
            set_error("Invalid or null path");
            return std::ptr::null_mut();
        }
    };

    let ps = match loka_core::PersistentStore::open(path_str) {
        Ok(ps) => ps,
        Err(e) => {
            set_error(&format!("Failed to open database: {}", e));
            return std::ptr::null_mut();
        }
    };

    let mut dict = loka_core::TermDictionary::new();
    ps.load_terms_into(&mut dict);

    let mut store = loka_core::TripleStore::new();
    for triple in ps.iter() {
        let _ = store.insert(triple);
    }

    // Rebuild HNSW indexes from stored vector triples
    let mut vectors = loka_hnsw::VectorRegistry::new();
    let f32vec_suffix = "^^<http://loka.dev/f32vec>";
    for triple in store.iter() {
        if let Some(obj_str) = dict.resolve(triple.object) {
            if obj_str.contains(f32vec_suffix) {
                if let Some(start) = obj_str.find('"') {
                    let end = obj_str[start + 1..].find('"').map(|p| p + start + 1);
                    if let Some(end) = end {
                        let vec_str = &obj_str[start + 1..end];
                        let floats: Vec<f32> = vec_str
                            .split_whitespace()
                            .filter_map(|s| s.parse::<f32>().ok())
                            .collect();
                        if !floats.is_empty() {
                            let dims = floats.len();
                            if !vectors.has_index(triple.predicate) {
                                let config = loka_hnsw::VectorPredicateConfig {
                                    predicate_id: triple.predicate,
                                    dimensions: dims,
                                    m: 16,
                                    ef_construction: 200,
                                    metric: loka_hnsw::DistanceMetric::Cosine,
                                };
                                let _ = vectors.declare(config);
                            }
                            let _ = vectors.insert(triple.predicate, floats, triple.object);
                        }
                    }
                }
            }
        }
    }

    let db = LokaDb {
        inner: Arc::new(Mutex::new(DbInner {
            ps,
            store,
            dict,
            vectors,
        })),
    };

    Box::into_raw(Box::new(db))
}

/// Close a database and free the handle.
///
/// After this call, the handle is invalid. Passing it to any other function
/// is undefined behavior.
#[no_mangle]
pub extern "C" fn loka_db_close(db: *mut LokaDb) {
    if !db.is_null() {
        unsafe {
            let db = Box::from_raw(db);
            let flush_result = db.inner.lock().map(|inner| inner.ps.flush());
            drop(flush_result);
            drop(db);
        }
    }
}

// ─── Triple operations ───────────────────────────────────────────────────────

/// Get the total number of triples in the database.
#[no_mangle]
pub extern "C" fn loka_triple_count(db: *const LokaDb) -> u64 {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => return 0,
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(_) => return 0,
    };
    inner.store.len() as u64
}

/// Get the number of terms in the dictionary.
#[no_mangle]
pub extern "C" fn loka_term_count(db: *const LokaDb) -> u64 {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => return 0,
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(_) => return 0,
    };
    inner.dict.len() as u64
}

/// Intern a term into the persistent store and mirror it into the in-memory
/// dictionary, returning the id (or a formatted error string).
fn ffi_intern(inner: &mut DbInner, term: &str) -> std::result::Result<loka_core::TermId, String> {
    let id = inner
        .ps
        .intern(term)
        .map_err(|e| format!("Intern error: {}", e))?;
    inner.dict.insert_with_id(term, id);
    Ok(id)
}

/// Resolve one subject/object position that may be an RDF-star quoted
/// triple. For a quoted position the inner triple is interned + stored and
/// the reverse map registered (in both the persistent store and the
/// in-memory dictionary), mirroring the proto/CLI ingest paths so a
/// `<< s p o >>` position round-trips instead of being dropped.
fn ffi_resolve_pos(
    inner: &mut DbInner,
    plain: &str,
    quoted: &Option<(String, String, String)>,
) -> std::result::Result<loka_core::TermId, String> {
    match quoted {
        Some((is, ip, io)) => {
            let is_id = ffi_intern(inner, is)?;
            let ip_id = ffi_intern(inner, ip)?;
            let io_id = ffi_intern(inner, io)?;
            let inner_t = loka_core::Triple::new(is_id, ip_id, io_id);
            let _ = inner.ps.insert(inner_t);
            let _ = inner.store.insert(inner_t);
            inner.dict.register_quoted(is_id, ip_id, io_id);
            inner
                .ps
                .register_quoted(is_id, ip_id, io_id)
                .map_err(|e| format!("Quoted register error: {}", e))
        }
        None => ffi_intern(inner, plain),
    }
}

/// Insert triples in N-Triples format.
///
/// Returns the number of triples inserted, or -1 on error.
#[no_mangle]
pub extern "C" fn loka_insert_ntriples(db: *mut LokaDb, data: *const c_char) -> i64 {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => {
            set_error("Null database handle");
            return -1;
        }
    };
    let data_str = match unsafe_cstr_to_str(data) {
        Some(s) => s,
        None => {
            set_error("Invalid or null data");
            return -1;
        }
    };

    let mut inner = match db.inner.lock() {
        Ok(i) => i,
        Err(e) => {
            set_error(&format!("Lock error: {}", e));
            return -1;
        }
    };

    let mut inserted = 0i64;
    for line in data_str.lines() {
        // RDF-star aware (Bug B fix): this path used the non-star parser,
        // which returns the <<QUOTED_TRIPLE>> sentinel and silently drops
        // the inner triple for `<< s p o >> p o` input. Use the star parser
        // and mirror the proto/CLI ingest: intern + store the inner triple
        // and register the quoted reverse map.
        let parsed = match loka_core::parse_ntriples_star_line(line) {
            Some(t) => t,
            None => continue,
        };
        let s_id = match ffi_resolve_pos(&mut inner, &parsed.subject, &parsed.inner_subject) {
            Ok(id) => id,
            Err(e) => {
                set_error(&e);
                return -1;
            }
        };
        let p_id = match ffi_intern(&mut inner, &parsed.predicate) {
            Ok(id) => id,
            Err(e) => {
                set_error(&e);
                return -1;
            }
        };
        let o_id = match ffi_resolve_pos(&mut inner, &parsed.object, &parsed.inner_object) {
            Ok(id) => id,
            Err(e) => {
                set_error(&e);
                return -1;
            }
        };
        let triple = loka_core::Triple::new(s_id, p_id, o_id);
        if inner.ps.insert(triple).is_ok() {
            let _ = inner.store.insert(triple);
            // Live HNSW indexing: a loka:f32vec object becomes
            // VECTOR_SIMILAR-searchable immediately, no reopen required
            // (previously only the loka_db_open rebuild picked vectors up).
            index_vector_literal(&mut inner, p_id, o_id, &parsed.object);
            inserted += 1;
        }
    }

    if let Err(e) = inner.ps.flush() {
        set_error(&format!("Flush error: {}", e));
        return -1;
    }

    inserted
}

// ─── Term dictionary ─────────────────────────────────────────────────────────

/// Intern a term string and return its ID. Returns 0 on error.
#[no_mangle]
pub extern "C" fn loka_intern(db: *mut LokaDb, term: *const c_char) -> u64 {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => return 0,
    };
    let term_str = match unsafe_cstr_to_str(term) {
        Some(s) => s,
        None => return 0,
    };
    let mut inner = match db.inner.lock() {
        Ok(i) => i,
        Err(_) => return 0,
    };
    match inner.ps.intern(term_str) {
        Ok(id) => {
            inner.dict.insert_with_id(term_str, id);
            id
        }
        Err(_) => 0,
    }
}

/// Resolve a term ID to its string representation.
///
/// Returns a C string that must be freed with `loka_string_free`.
/// Returns null if the ID is unknown.
#[no_mangle]
pub extern "C" fn loka_resolve(db: *const LokaDb, id: u64) -> *mut c_char {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => return std::ptr::null_mut(),
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(_) => return std::ptr::null_mut(),
    };

    // Check inline types first
    if let Some(n) = loka_core::decode_inline_integer(id) {
        return string_to_c(&n.to_string());
    }
    if let Some(b) = loka_core::decode_inline_boolean(id) {
        return string_to_c(&b.to_string());
    }

    // render_term also reverses RDF-star quoted-triple ids to `<< s p o >>`.
    inner
        .dict
        .render_term(id)
        .map(|s| string_to_c(&s))
        .unwrap_or(std::ptr::null_mut())
}

// ─── SPARQL query ────────────────────────────────────────────────────────────

/// Execute a SPARQL+ query and return a result handle.
///
/// Returns null on error (call `loka_last_error()`).
/// The result must be freed with `loka_result_free`.
#[no_mangle]
pub extern "C" fn loka_query(db: *const LokaDb, query: *const c_char) -> *mut LokaResult {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => {
            set_error("Null database handle");
            return std::ptr::null_mut();
        }
    };
    let query_str = match unsafe_cstr_to_str(query) {
        Some(s) => s,
        None => {
            set_error("Invalid or null query");
            return std::ptr::null_mut();
        }
    };

    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(e) => {
            set_error(&format!("Lock error: {}", e));
            return std::ptr::null_mut();
        }
    };

    let mut parsed = match loka_sparql::parse(query_str) {
        Ok(q) => q,
        Err(e) => {
            set_error(&format!("SPARQL parse error: {}", e));
            return std::ptr::null_mut();
        }
    };

    loka_sparql::optimize(&mut parsed);

    let result =
        match loka_sparql::execute_with_vectors(&parsed, &inner.store, &inner.dict, &inner.vectors)
        {
            Ok(r) => r,
            Err(e) => {
                set_error(&format!("SPARQL execution error: {}", e));
                return std::ptr::null_mut();
            }
        };

    // Convert to string-resolved rows for easy consumption across FFI
    let columns = result.columns.clone();
    let rows: Vec<Vec<String>> = result
        .rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|col| {
                    row.get(col)
                        .map(|&id| resolve_id(id, &inner.dict))
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();

    Box::into_raw(Box::new(LokaResult { columns, rows }))
}

/// Get the number of columns in a result.
#[no_mangle]
pub extern "C" fn loka_result_column_count(result: *const LokaResult) -> u32 {
    if result.is_null() {
        return 0;
    }
    unsafe { (*result).columns.len() as u32 }
}

/// Get the number of rows in a result.
#[no_mangle]
pub extern "C" fn loka_result_row_count(result: *const LokaResult) -> u64 {
    if result.is_null() {
        return 0;
    }
    unsafe { (*result).rows.len() as u64 }
}

/// Get a column name by index.
///
/// Returns a C string that must be freed with `loka_string_free`.
#[no_mangle]
pub extern "C" fn loka_result_column_name(result: *const LokaResult, index: u32) -> *mut c_char {
    if result.is_null() {
        return std::ptr::null_mut();
    }
    let result = unsafe { &*result };
    result
        .columns
        .get(index as usize)
        .map(|s| string_to_c(s))
        .unwrap_or(std::ptr::null_mut())
}

/// Get a cell value by row and column index.
///
/// Returns a C string that must be freed with `loka_string_free`.
#[no_mangle]
pub extern "C" fn loka_result_value(result: *const LokaResult, row: u64, col: u32) -> *mut c_char {
    if result.is_null() {
        return std::ptr::null_mut();
    }
    let result = unsafe { &*result };
    result
        .rows
        .get(row as usize)
        .and_then(|r| r.get(col as usize))
        .map(|s| string_to_c(s))
        .unwrap_or(std::ptr::null_mut())
}

/// Free a query result handle.
#[no_mangle]
pub extern "C" fn loka_result_free(result: *mut LokaResult) {
    if !result.is_null() {
        unsafe {
            drop(Box::from_raw(result));
        }
    }
}

// ─── Health diagnostics ──────────────────────────────────────────────────────

/// Generate a full health report as structured text.
///
/// Returns a C string that must be freed with `loka_string_free`.
#[no_mangle]
pub extern "C" fn loka_health_report(db: *const LokaDb) -> *mut c_char {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => {
            set_error("Null database handle");
            return std::ptr::null_mut();
        }
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(e) => {
            set_error(&format!("Lock error: {}", e));
            return std::ptr::null_mut();
        }
    };

    let report =
        loka_sparql::generate_health_report(&inner.store, &inner.dict, &inner.vectors, None);
    string_to_c(&report.to_ai_text())
}

/// Get the overall health status: 0 = healthy, 1 = warning, 2 = critical.
#[no_mangle]
pub extern "C" fn loka_health_status(db: *const LokaDb) -> i32 {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => return -1,
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(_) => return -1,
    };
    let report =
        loka_sparql::generate_health_report(&inner.store, &inner.dict, &inner.vectors, None);
    match report.overall_status {
        loka_sparql::HealthStatus::Healthy => 0,
        loka_sparql::HealthStatus::Warning => 1,
        loka_sparql::HealthStatus::Critical => 2,
    }
}

// ─── Database info ───────────────────────────────────────────────────────────

/// Get database info as a JSON string.
///
/// Returns a C string that must be freed with `loka_string_free`.
#[no_mangle]
pub extern "C" fn loka_db_info(db: *const LokaDb) -> *mut c_char {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => {
            set_error("Null database handle");
            return std::ptr::null_mut();
        }
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(e) => {
            set_error(&format!("Lock error: {}", e));
            return std::ptr::null_mut();
        }
    };

    let info = format!(
        "{{\"triples\":{},\"terms\":{},\"vector_predicates\":{}}}",
        inner.store.len(),
        inner.dict.len(),
        inner.vectors.predicates().len()
    );
    string_to_c(&info)
}

// ─── Index consistency ───────────────────────────────────────────────────────

/// Verify SPO/POS/OSP index consistency. Returns 1 if consistent, 0 if not.
#[no_mangle]
pub extern "C" fn loka_verify_consistency(db: *const LokaDb) -> i32 {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => return -1,
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(_) => return -1,
    };
    if inner.ps.verify_consistency() {
        1
    } else {
        0
    }
}

/// Repair index inconsistencies. Returns the number of triples repaired, or -1 on error.
#[no_mangle]
pub extern "C" fn loka_repair(db: *mut LokaDb) -> i64 {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => {
            set_error("Null database handle");
            return -1;
        }
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(e) => {
            set_error(&format!("Lock error: {}", e));
            return -1;
        }
    };
    match inner.ps.repair() {
        Ok(count) => {
            let _ = inner.ps.flush();
            count as i64
        }
        Err(e) => {
            set_error(&format!("Repair failed: {}", e));
            -1
        }
    }
}

// ─── Export ───────────────────────────────────────────────────────────────────

/// Export all triples as N-Triples text.
///
/// Returns a C string that must be freed with `loka_string_free`.
#[no_mangle]
pub extern "C" fn loka_export_ntriples(db: *const LokaDb) -> *mut c_char {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => {
            set_error("Null database handle");
            return std::ptr::null_mut();
        }
    };
    let inner = match db.inner.lock() {
        Ok(i) => i,
        Err(e) => {
            set_error(&format!("Lock error: {}", e));
            return std::ptr::null_mut();
        }
    };

    let mut output = String::new();
    for triple in inner.store.iter() {
        let s = resolve_id(triple.subject, &inner.dict);
        let p = resolve_id(triple.predicate, &inner.dict);
        let o = resolve_id(triple.object, &inner.dict);
        output.push_str(&format!("{} {} {} .\n", s, p, o));
    }
    string_to_c(&output)
}

// ─── String memory management ────────────────────────────────────────────────

/// Free a C string returned by any `loka_*` function.
///
/// Must be called on every non-null string returned by this library.
#[no_mangle]
pub extern "C" fn loka_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

// ─── Version ─────────────────────────────────────────────────────────────────

/// Get the Loka version string.
///
/// Returns a static string — do NOT free it.
#[no_mangle]
pub extern "C" fn loka_version() -> *const c_char {
    // This is a static string, no need to free
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

// ─── Cascade retraction ──────────────────────────────────────────────────────

/// Cascade-retraction on the live handle: remove a node — real data OR
/// AI-generated — and every generated inference that transitively cited it.
/// Propagation follows ONLY `http://loka.dev/provenance/propositionInferredFrom`
/// back-edges (`loka_core::retract_set`); ordinary data edges are never
/// followed.
///
/// `commit == false` is a non-destructive preview. Returns a JSON object
/// `{"root","total","maxDepth","committed","removed","triples":[{"depth","s","p","o"},…]}`
/// as a C string that must be freed with `loka_string_free`; null on error
/// (call `loka_last_error()`). An unknown IRI yields an empty set, not an
/// error. On commit, removed vector rows are also tombstoned in the live
/// HNSW registry.
#[no_mangle]
pub extern "C" fn loka_retract_node(
    db: *mut LokaDb,
    iri: *const c_char,
    commit: bool,
) -> *mut c_char {
    let db = match unsafe_db_ref(db) {
        Some(d) => d,
        None => {
            set_error("Null database handle");
            return std::ptr::null_mut();
        }
    };
    let iri_str = match unsafe_cstr_to_str(iri) {
        Some(s) => s,
        None => {
            set_error("Invalid or null iri");
            return std::ptr::null_mut();
        }
    };
    let mut inner = match db.inner.lock() {
        Ok(i) => i,
        Err(e) => {
            set_error(&format!("Lock error: {}", e));
            return std::ptr::null_mut();
        }
    };

    // RetractSet owns its triples (Triple is Copy), so no borrow of the
    // store survives past this call — the commit loop below may mutate.
    let set = match inner.dict.lookup(iri_str) {
        Some(root) => loka_core::retract_set(root, &inner.store, &inner.dict),
        None => loka_core::RetractSet::default(),
    };

    let mut triples_json: Vec<String> = Vec::new();
    for (depth, triples) in set.by_depth.iter().enumerate() {
        for t in triples {
            triples_json.push(format!(
                "{{\"depth\":{},\"s\":\"{}\",\"p\":\"{}\",\"o\":\"{}\"}}",
                depth,
                json_escape(&resolve_id(t.subject, &inner.dict)),
                json_escape(&resolve_id(t.predicate, &inner.dict)),
                json_escape(&resolve_id(t.object, &inner.dict)),
            ));
        }
    }

    let mut removed = 0usize;
    if commit {
        let doomed: Vec<loka_core::Triple> = set.all().copied().collect();
        for t in &doomed {
            if inner.ps.remove(t).unwrap_or(false) {
                removed += 1;
            }
            inner.store.remove(t);
            // Tombstone any vector object in the live registry (no-op for
            // non-vector triples). Reopen rebuilds cleanly either way.
            inner.vectors.delete(t.predicate, t.object);
        }
        if let Err(e) = inner.ps.flush() {
            set_error(&format!("Flush error: {}", e));
            return std::ptr::null_mut();
        }
    }

    string_to_c(&format!(
        "{{\"root\":\"{}\",\"total\":{},\"maxDepth\":{},\"committed\":{},\"removed\":{},\"triples\":[{}]}}",
        json_escape(iri_str),
        set.total(),
        set.max_depth(),
        commit,
        removed,
        triples_json.join(","),
    ))
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// If an inserted object literal is a `loka:f32vec`, feed the live HNSW
/// registry (auto-declare on first use: dims from data, M=16, cosine) so the
/// vector is searchable without a reopen. Mirrors the `loka_db_open` rebuild
/// scan; parse failures are ignored (the row still stores as a plain triple,
/// exactly as before this hook existed).
fn index_vector_literal(
    inner: &mut DbInner,
    predicate_id: loka_core::TermId,
    object_id: loka_core::TermId,
    obj_str: &str,
) {
    if !obj_str.contains("^^<http://loka.dev/f32vec>") {
        return;
    }
    let Some(start) = obj_str.find('"') else {
        return;
    };
    let Some(end) = obj_str[start + 1..].find('"').map(|p| p + start + 1) else {
        return;
    };
    let floats: Vec<f32> = obj_str[start + 1..end]
        .split_whitespace()
        .filter_map(|s| s.parse::<f32>().ok())
        .collect();
    if floats.is_empty() {
        return;
    }
    if !inner.vectors.has_index(predicate_id) {
        let config = loka_hnsw::VectorPredicateConfig {
            predicate_id,
            dimensions: floats.len(),
            m: 16,
            ef_construction: 200,
            metric: loka_hnsw::DistanceMetric::Cosine,
        };
        let _ = inner.vectors.declare(config);
    }
    let _ = inner.vectors.insert(predicate_id, floats, object_id);
}

/// Minimal JSON string escaper (quotes, backslashes, control chars). Rust's
/// `{:?}` is NOT JSON-safe (it emits `\u{XXXX}` with braces).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn unsafe_cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr).to_str().ok() }
}

fn unsafe_db_ref<'a>(ptr: *const LokaDb) -> Option<&'a LokaDb> {
    if ptr.is_null() {
        None
    } else {
        unsafe { Some(&*ptr) }
    }
}

fn string_to_c(s: &str) -> *mut c_char {
    CString::new(s)
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn resolve_id(id: loka_core::TermId, dict: &loka_core::TermDictionary) -> String {
    // render_term resolves plain terms exactly as `resolve` would and an
    // RDF-star quoted-triple id to faithful `<< s p o >>` (no more _:idN).
    dict.render_term(id)
        .unwrap_or_else(|| format!("_:id{}", id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sdb");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();

        let db = loka_db_open(path_c.as_ptr());
        assert!(!db.is_null());
        assert_eq!(loka_triple_count(db), 0);
        loka_db_close(db);
    }

    #[test]
    fn test_insert_and_query() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sdb");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();

        let db = loka_db_open(path_c.as_ptr());
        assert!(!db.is_null());

        let data = CString::new(
            "<http://example.org/Alice> <http://example.org/knows> <http://example.org/Bob> .",
        )
        .unwrap();
        let inserted = loka_insert_ntriples(db, data.as_ptr());
        assert_eq!(inserted, 1);
        assert_eq!(loka_triple_count(db), 1);

        let query = CString::new("SELECT ?s ?p ?o WHERE { ?s ?p ?o }").unwrap();
        let result = loka_query(db, query.as_ptr());
        assert!(!result.is_null());
        assert_eq!(loka_result_row_count(result), 1);
        assert_eq!(loka_result_column_count(result), 3);

        loka_result_free(result);
        loka_db_close(db);
    }

    #[test]
    fn test_rdf_star_insert_round_trips_not_dropped() {
        // Bug B regression: the FFI insert path used the non-star parser,
        // which dropped the inner triple and interned the
        // <<QUOTED_TRIPLE>> sentinel. Now it must store the inner triple
        // AND render the quoted subject faithfully.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("star.sdb");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();
        let db = loka_db_open(path_c.as_ptr());
        assert!(!db.is_null());

        let data = CString::new(
            "<< <http://wd/Q42> <http://wd/P20> <http://wd/Q31> >> \
             <http://loka.dev/provenance/propositionConfidence> \"0.9\" .",
        )
        .unwrap();
        let inserted = loka_insert_ntriples(db, data.as_ptr());
        assert_eq!(inserted, 1, "the annotation row");
        // 2 triples on disk: the inner asserted triple + the annotation.
        // (Pre-fix this was 1 — the inner triple was silently lost.)
        assert_eq!(loka_triple_count(db), 2);

        let query = CString::new(
            "SELECT ?s ?v WHERE { ?s \
             <http://loka.dev/provenance/propositionConfidence> ?v }",
        )
        .unwrap();
        let result = loka_query(db, query.as_ptr());
        assert!(!result.is_null());
        assert_eq!(loka_result_row_count(result), 1);

        let sval = loka_result_value(result, 0, 0);
        assert!(!sval.is_null());
        let s = unsafe { CStr::from_ptr(sval) }.to_str().unwrap();
        assert_eq!(
            s, "<< <http://wd/Q42> <http://wd/P20> <http://wd/Q31> >>",
            "quoted subject renders faithfully, not _:idN / sentinel"
        );

        loka_result_free(result);
        loka_db_close(db);
    }

    #[test]
    fn test_version() {
        let v = loka_version();
        assert!(!v.is_null());
        let version = unsafe { CStr::from_ptr(v) }.to_str().unwrap();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_vector_insert_searchable_without_reopen() {
        // A+ decision (Topaz 2026-07-26): insert_ntriples feeds the live
        // HNSW registry, so VECTOR_SIMILAR works on the same handle.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vec.sdb");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();
        let db = loka_db_open(path_c.as_ptr());
        assert!(!db.is_null());

        let data = CString::new(
            "<http://t/a> <http://t/emb> \"1 0 0 0\"^^<http://loka.dev/f32vec> .\n\
             <http://t/b> <http://t/emb> \"0 1 0 0\"^^<http://loka.dev/f32vec> .",
        )
        .unwrap();
        assert_eq!(loka_insert_ntriples(db, data.as_ptr()), 2);

        let query = CString::new(
            "SELECT ?s WHERE { VECTOR_SIMILAR(?s <http://t/emb> \
             \"0.9 0.1 0 0\"^^<http://loka.dev/f32vec>, 0.5) }",
        )
        .unwrap();
        let result = loka_query(db, query.as_ptr());
        assert!(!result.is_null(), "query failed without reopen");
        let rows = loka_result_row_count(result);
        assert!(rows >= 1, "expected the near vector, got {} rows", rows);
        let sval = loka_result_value(result, 0, 0);
        let s = unsafe { CStr::from_ptr(sval) }.to_str().unwrap().to_string();
        loka_string_free(sval);
        assert_eq!(s, "http://t/a");
        loka_result_free(result);
        loka_db_close(db);
    }

    #[test]
    fn test_retract_node_cascade() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("retract.sdb");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();
        let db = loka_db_open(path_c.as_ptr());
        assert!(!db.is_null());

        // Real fact + a generated inference citing it (star provenance).
        let data = CString::new(
            "<http://t/f> <http://t/p> <http://t/x> .\n\
             <http://t/g> <http://t/p> <http://t/y> .\n\
             << <http://t/g> <http://t/p> <http://t/y> >> \
             <http://loka.dev/provenance/propositionInferredFrom> \
             << <http://t/f> <http://t/p> <http://t/x> >> .",
        )
        .unwrap();
        assert_eq!(loka_insert_ntriples(db, data.as_ptr()), 3);
        let before = loka_triple_count(db);

        // Preview: non-destructive, must include the generated row at depth>=1.
        let iri = CString::new("http://t/f").unwrap();
        let preview_ptr = loka_retract_node(db, iri.as_ptr(), false);
        assert!(!preview_ptr.is_null());
        let preview = unsafe { CStr::from_ptr(preview_ptr) }.to_str().unwrap().to_string();
        loka_string_free(preview_ptr);
        assert!(preview.contains("\"committed\":false"), "{}", preview);
        assert!(preview.contains("http://t/g"), "cascade reaches the inference: {}", preview);
        assert_eq!(loka_triple_count(db), before, "preview must not mutate");

        // Commit: root row + generated row + its provenance annotation go.
        let commit_ptr = loka_retract_node(db, iri.as_ptr(), true);
        assert!(!commit_ptr.is_null());
        let committed = unsafe { CStr::from_ptr(commit_ptr) }.to_str().unwrap().to_string();
        loka_string_free(commit_ptr);
        assert!(committed.contains("\"committed\":true"), "{}", committed);
        assert!(loka_triple_count(db) < before);

        // The generated inference must be gone from queries.
        let q = CString::new("SELECT ?o WHERE { <http://t/g> <http://t/p> ?o }").unwrap();
        let result = loka_query(db, q.as_ptr());
        assert!(!result.is_null());
        assert_eq!(loka_result_row_count(result), 0, "inference evaporated");
        loka_result_free(result);

        // Unknown IRI: empty set, not an error.
        let nope = CString::new("http://t/never-seen").unwrap();
        let empty_ptr = loka_retract_node(db, nope.as_ptr(), false);
        assert!(!empty_ptr.is_null());
        let empty = unsafe { CStr::from_ptr(empty_ptr) }.to_str().unwrap().to_string();
        loka_string_free(empty_ptr);
        assert!(empty.contains("\"total\":0"), "{}", empty);

        loka_db_close(db);
    }
}
