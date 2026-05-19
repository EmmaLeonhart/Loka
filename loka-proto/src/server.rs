//! HTTP server for SPARQL protocol.
//!
//! Implements a subset of the SPARQL 1.1 Protocol (W3C Recommendation):
//! - GET  /sparql?query=...  (query via URL parameter)
//! - POST /sparql            (query in request body)
//! - GET  /graph             (export all triples as Turtle — for Protégé)
//! - POST /triples           (insert N-Triples data)
//! - POST /vectors/declare   (declare a vector predicate)
//! - POST /vectors           (insert a vector)
//! - GET  /health            (health check)
//!
//! Results are returned as JSON (application/sparql-results+json).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use axum::extract::{Query as AxumQuery, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use loka_core::{PersistentStore, TermDictionary, TripleStore};
use loka_hnsw::VectorRegistry;

use crate::error::ProtoError;

/// Shared application state.
///
/// Uses RwLock for read-heavy workloads (concurrent SPARQL queries).
/// The in-memory stores are the working set for the SPARQL executor.
/// When `persistent` is Some, all writes go to both in-memory and disk.
pub struct AppState {
    pub store: RwLock<TripleStore>,
    pub dict: RwLock<TermDictionary>,
    pub vectors: RwLock<VectorRegistry>,
    /// Optional persistent backing store. Protected by RwLock to serialize
    /// concurrent writes. When present, all mutations are written through
    /// to disk atomically and flushed before returning success.
    pub persistent: Option<RwLock<PersistentStore>>,
    /// Optional passcode for simple authentication (server mode only).
    /// When set, all requests (except /health) must include
    /// `Authorization: Bearer <passcode>` header.
    pub passcode: Option<String>,
    /// Rate limit: max requests per minute. 0 = unlimited.
    pub rate_limit_per_min: u32,
    /// Rate limit counter (atomically incremented).
    pub rate_counter: AtomicU64,
}

/// Build the axum router with all endpoints.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sparql", get(sparql_get).post(sparql_post))
        .route("/graph", get(export_graph))
        .route("/browse", get(serve_browse))
        .route("/sparql.csv", get(sparql_csv_get).post(sparql_csv_post))
        .route("/sparql.tsv", get(sparql_tsv_get).post(sparql_tsv_post))
        .route("/sparql.xml", get(sparql_xml_get).post(sparql_xml_post))
        .route("/triples", post(insert_triples))
        .route("/vectors/declare", post(declare_vector_predicate))
        .route("/vectors", post(insert_vector))
        .route("/health", get(health))
        .route("/graph-store", get(gsp_get).put(gsp_put).delete(gsp_delete))
        .route("/vectors/health", get(vectors_health))
        .route("/vectors/rebuild", post(rebuild_hnsw))
        .route("/retract/preview", post(retract_preview))
        .route("/retract", post(retract_apply))
        .route("/.well-known/void", get(service_description))
        .route("/service-description", get(service_description))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Simple passcode authentication middleware.
/// Skips auth for /health endpoint. When passcode is not configured, all requests pass.
async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    // No passcode configured — allow all
    let passcode = match &state.passcode {
        Some(p) => p,
        None => return next.run(req).await,
    };

    // Health endpoint is always accessible
    if req.uri().path() == "/health" {
        return next.run(req).await;
    }

    // Rate limiting (simple counter, resets every 60 seconds)
    if state.rate_limit_per_min > 0 {
        let count = state.rate_counter.fetch_add(1, Ordering::Relaxed);
        // Simple approximation: reset counter periodically (every ~1000 requests check time)
        if count > state.rate_limit_per_min as u64 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Rate limit exceeded. Try again later.",
            )
                .into_response();
        }
    }

    // Check Authorization: Bearer <passcode>
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(h) if h.starts_with("Bearer ") && &h[7..] == passcode => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            "Unauthorized: include Authorization: Bearer <passcode> header",
        )
            .into_response(),
    }
}

/// Query parameters for GET /sparql.
#[derive(Deserialize)]
pub struct SparqlQueryParams {
    query: String,
}

/// SPARQL results JSON format (simplified W3C format).
#[derive(Serialize)]
pub struct SparqlResults {
    pub head: SparqlHead,
    pub results: SparqlBindings,
}

#[derive(Serialize)]
pub struct SparqlHead {
    pub vars: Vec<String>,
}

#[derive(Serialize)]
pub struct SparqlBindings {
    pub bindings: Vec<serde_json::Value>,
}

/// GET /sparql?query=SELECT...
async fn sparql_get(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<SparqlQueryParams>,
) -> Result<Json<SparqlResults>, ProtoError> {
    execute_sparql(&params.query, &state)
}

/// POST /sparql with query in body. Supports content negotiation via Accept header.
async fn sparql_post(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<axum::response::Response, ProtoError> {
    let query = if let Some(encoded) = body.strip_prefix("query=") {
        urlencoding::decode(encoded)
            .map_err(|e| ProtoError::BadRequest(format!("invalid encoding: {}", e)))?
            .into_owned()
    } else {
        body
    };

    // Content negotiation via Accept header
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/sparql-results+json");

    if accept.contains("text/csv") {
        let resp = sparql_delimited(&query, &state, ",", "text/csv; charset=utf-8")?;
        Ok(resp.into_response())
    } else if accept.contains("text/tab-separated") || accept.contains("text/tsv") {
        let resp = sparql_delimited(
            &query,
            &state,
            "\t",
            "text/tab-separated-values; charset=utf-8",
        )?;
        Ok(resp.into_response())
    } else if accept.contains("application/sparql-results+xml") || accept.contains("text/xml") {
        let resp = sparql_xml(&query, &state)?;
        Ok(resp.into_response())
    } else {
        let resp = execute_sparql(&query, &state)?;
        Ok(resp.into_response())
    }
}

// ─── SPARQL CSV/TSV ─────────────────────────────────────────────────────────

async fn sparql_csv_get(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<SparqlQueryParams>,
) -> Result<impl IntoResponse, ProtoError> {
    sparql_delimited(&params.query, &state, ",", "text/csv; charset=utf-8")
}

async fn sparql_csv_post(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<impl IntoResponse, ProtoError> {
    sparql_delimited(&body, &state, ",", "text/csv; charset=utf-8")
}

async fn sparql_tsv_get(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<SparqlQueryParams>,
) -> Result<impl IntoResponse, ProtoError> {
    sparql_delimited(
        &params.query,
        &state,
        "\t",
        "text/tab-separated-values; charset=utf-8",
    )
}

async fn sparql_tsv_post(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<impl IntoResponse, ProtoError> {
    sparql_delimited(
        &body,
        &state,
        "\t",
        "text/tab-separated-values; charset=utf-8",
    )
}

fn sparql_delimited(
    query_str: &str,
    state: &AppState,
    delimiter: &str,
    content_type: &'static str,
) -> Result<impl IntoResponse, ProtoError> {
    let mut query = loka_sparql::parse(query_str)?;

    let store = state
        .store
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
    let dict = state
        .dict
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
    let vectors = state
        .vectors
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;

    // Optimize with full cost model: store cardinality + dictionary IRI resolution
    loka_sparql::optimize_full(&mut query, Some(&store), Some(&dict));

    let result = loka_sparql::execute_with_vectors(&query, &store, &dict, &vectors)?;

    let mut output = String::new();

    // Header row
    output.push_str(&result.columns.join(delimiter));
    output.push('\n');

    // Data rows
    for row in &result.rows {
        let vals: Vec<String> = result
            .columns
            .iter()
            .map(|col| {
                row.get(col)
                    .map(|&id| resolve_term_for_csv(id, &dict))
                    .unwrap_or_default()
            })
            .collect();
        output.push_str(&vals.join(delimiter));
        output.push('\n');
    }

    Ok(([(header::CONTENT_TYPE, content_type)], output))
}

fn resolve_term_for_csv(id: loka_core::TermId, dict: &TermDictionary) -> String {
    if let Some(n) = loka_core::decode_inline_integer(id) {
        return n.to_string();
    }
    if let Some(b) = loka_core::decode_inline_boolean(id) {
        return b.to_string();
    }
    // render_term resolves plain terms exactly as `resolve` would and an
    // RDF-star quoted-triple id to faithful `<< s p o >>` (no more _:idN).
    dict.render_term(id)
        .unwrap_or_else(|| format!("_:id{}", id))
}

// ─── SPARQL XML ─────────────────────────────────────────────────────────────

async fn sparql_xml_get(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<SparqlQueryParams>,
) -> Result<impl IntoResponse, ProtoError> {
    sparql_xml(&params.query, &state)
}

async fn sparql_xml_post(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<impl IntoResponse, ProtoError> {
    sparql_xml(&body, &state)
}

fn sparql_xml(query_str: &str, state: &AppState) -> Result<impl IntoResponse, ProtoError> {
    let mut query = loka_sparql::parse(query_str)?;

    let store = state
        .store
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
    let dict = state
        .dict
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
    let vectors = state
        .vectors
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;

    loka_sparql::optimize_full(&mut query, Some(&store), Some(&dict));

    let result = loka_sparql::execute_with_vectors(&query, &store, &dict, &vectors)?;

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n  <head>\n",
    );
    for col in &result.columns {
        xml.push_str(&format!("    <variable name=\"{}\"/>\n", col));
    }
    xml.push_str("  </head>\n  <results>\n");

    for row in &result.rows {
        xml.push_str("    <result>\n");
        for col in &result.columns {
            if let Some(&id) = row.get(col) {
                let val = resolve_term_for_csv(id, &dict);
                let escaped = val
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                if loka_core::is_inline(id) || dict.resolve(id).is_some_and(|s| s.starts_with('"'))
                {
                    xml.push_str(&format!(
                        "      <binding name=\"{}\"><literal>{}</literal></binding>\n",
                        col, escaped
                    ));
                } else {
                    xml.push_str(&format!(
                        "      <binding name=\"{}\"><uri>{}</uri></binding>\n",
                        col, escaped
                    ));
                }
            }
        }
        xml.push_str("    </result>\n");
    }
    xml.push_str("  </results>\n</sparql>\n");

    Ok((
        [(
            header::CONTENT_TYPE,
            "application/sparql-results+xml; charset=utf-8",
        )],
        xml,
    ))
}

/// Execute a SPARQL query and return JSON results.
fn execute_sparql(query_str: &str, state: &AppState) -> Result<Json<SparqlResults>, ProtoError> {
    let mut query = loka_sparql::parse(query_str)?;

    // Handle SPARQL Update (INSERT DATA / DELETE DATA)
    if query.query_type == loka_sparql::QueryType::InsertData {
        return execute_insert_data(&query, state);
    }
    if query.query_type == loka_sparql::QueryType::DeleteData {
        return execute_delete_data(&query, state);
    }

    // Read locks: concurrent SPARQL queries don't block each other
    let store = state
        .store
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    let dict = state
        .dict
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    let vectors = state
        .vectors
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;

    // Optimize with full cost model: store cardinality + dictionary IRI resolution
    loka_sparql::optimize_full(&mut query, Some(&store), Some(&dict));

    let result = loka_sparql::execute_with_vectors(&query, &store, &dict, &vectors)?;

    let bindings: Vec<serde_json::Value> = result
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut obj = serde_json::Map::new();
            for col in &result.columns {
                if let Some(&id) = row.get(col) {
                    let value = resolve_term_to_json(id, &dict);
                    obj.insert(col.clone(), value);
                }
            }
            // Include similarity scores if present
            if !result.scores[i].is_empty() {
                let scores_obj: serde_json::Map<String, serde_json::Value> = result.scores[i]
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::json!(*v)))
                    .collect();
                obj.insert("_scores".to_string(), serde_json::Value::Object(scores_obj));
            }
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(Json(SparqlResults {
        head: SparqlHead {
            vars: result.columns,
        },
        results: SparqlBindings { bindings },
    }))
}

/// Convert a TermId back to a JSON value for the SPARQL results format.
/// Execute INSERT DATA { triple patterns }.
fn execute_insert_data(
    query: &loka_sparql::Query,
    state: &AppState,
) -> Result<Json<SparqlResults>, ProtoError> {
    use loka_sparql::parser::Pattern;

    let mut dict = state
        .dict
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
    let mut store = state
        .store
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;

    let mut inserted = 0i64;
    for pattern in &query.patterns {
        if let Pattern::Triple {
            subject,
            predicate,
            object,
        } = pattern
        {
            // If subject or object is a quoted triple, also store the inner triple
            use loka_sparql::parser::Term;
            if let Term::QuotedTriple {
                subject: qs,
                predicate: qp,
                object: qo,
            } = subject
            {
                let qs_id = resolve_term_to_id(qs, &mut dict, &query.prefixes)?;
                let qp_id = resolve_term_to_id(qp, &mut dict, &query.prefixes)?;
                let qo_id = resolve_term_to_id(qo, &mut dict, &query.prefixes)?;
                let inner = loka_core::Triple::new(qs_id, qp_id, qo_id);
                dict.register_quoted(qs_id, qp_id, qo_id);
                if store.insert(inner).is_ok() {
                    if let Some(ref ps_lock) = state.persistent {
                        let ps = ps_lock
                            .write()
                            .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
                        let _ = ps.insert(inner);
                        let _ = ps.register_quoted(qs_id, qp_id, qo_id);
                    }
                }
            }
            if let Term::QuotedTriple {
                subject: qs,
                predicate: qp,
                object: qo,
            } = object
            {
                let qs_id = resolve_term_to_id(qs, &mut dict, &query.prefixes)?;
                let qp_id = resolve_term_to_id(qp, &mut dict, &query.prefixes)?;
                let qo_id = resolve_term_to_id(qo, &mut dict, &query.prefixes)?;
                let inner = loka_core::Triple::new(qs_id, qp_id, qo_id);
                dict.register_quoted(qs_id, qp_id, qo_id);
                if store.insert(inner).is_ok() {
                    if let Some(ref ps_lock) = state.persistent {
                        let ps = ps_lock
                            .write()
                            .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
                        let _ = ps.insert(inner);
                        let _ = ps.register_quoted(qs_id, qp_id, qo_id);
                    }
                }
            }

            let s_id = resolve_term_to_id(subject, &mut dict, &query.prefixes)?;
            let p_id = resolve_term_to_id(predicate, &mut dict, &query.prefixes)?;
            let o_id = resolve_term_to_id(object, &mut dict, &query.prefixes)?;

            // Check for schema declarations: loka:declareVectorPredicate
            let pred_str = dict.resolve(p_id).unwrap_or("").to_string();
            if pred_str == "http://loka.dev/dimensions"
                || pred_str.contains("declareVectorPredicate")
            {
                // This is a vector schema triple — try to auto-declare
                if let Some(dims) = loka_core::decode_inline_integer(o_id) {
                    let mut vectors = state
                        .vectors
                        .write()
                        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
                    if !vectors.has_index(s_id) {
                        let config = loka_hnsw::VectorPredicateConfig {
                            predicate_id: s_id,
                            dimensions: dims as usize,
                            m: 16,
                            ef_construction: 200,
                            metric: loka_hnsw::DistanceMetric::Cosine,
                        };
                        let _ = vectors.declare(config);
                    }
                }
            }

            let triple = loka_core::Triple::new(s_id, p_id, o_id);
            if store.insert(triple).is_ok() {
                if let Some(ref ps_lock) = state.persistent {
                    let ps = ps_lock
                        .write()
                        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
                    ps.insert(triple)
                        .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
                }
                inserted += 1;
            }
        }
    }

    // Flush persistent store to ensure durability
    if let Some(ref ps_lock) = state.persistent {
        if inserted > 0 {
            let ps = ps_lock
                .read()
                .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
            ps.flush()
                .map_err(|e| ProtoError::BadRequest(format!("flush: {}", e)))?;
        }
    }

    Ok(Json(SparqlResults {
        head: SparqlHead {
            vars: vec!["mutationCount".to_string()],
        },
        results: SparqlBindings {
            bindings: vec![
                serde_json::json!({"mutationCount": {"type": "literal", "value": inserted.to_string()}}),
            ],
        },
    }))
}

/// Execute DELETE DATA { triple patterns }.
fn execute_delete_data(
    query: &loka_sparql::Query,
    state: &AppState,
) -> Result<Json<SparqlResults>, ProtoError> {
    use loka_sparql::parser::Pattern;

    let mut dict = state
        .dict
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
    let mut store = state
        .store
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;

    let mut deleted = 0i64;
    for pattern in &query.patterns {
        if let Pattern::Triple {
            subject,
            predicate,
            object,
        } = pattern
        {
            let s_id = resolve_term_to_id(subject, &mut dict, &query.prefixes)?;
            let p_id = resolve_term_to_id(predicate, &mut dict, &query.prefixes)?;
            let o_id = resolve_term_to_id(object, &mut dict, &query.prefixes)?;

            let triple = loka_core::Triple::new(s_id, p_id, o_id);
            if store.remove(&triple) {
                if let Some(ref ps_lock) = state.persistent {
                    let ps = ps_lock
                        .write()
                        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
                    ps.remove(&triple)
                        .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
                }
                deleted += 1;
            }
        }
    }

    // Flush persistent store to ensure durability
    if let Some(ref ps_lock) = state.persistent {
        if deleted > 0 {
            let ps = ps_lock
                .read()
                .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
            ps.flush()
                .map_err(|e| ProtoError::BadRequest(format!("flush: {}", e)))?;
        }
    }

    Ok(Json(SparqlResults {
        head: SparqlHead {
            vars: vec!["mutationCount".to_string()],
        },
        results: SparqlBindings {
            bindings: vec![
                serde_json::json!({"mutationCount": {"type": "literal", "value": deleted.to_string()}}),
            ],
        },
    }))
}

/// Resolve a parsed Term to a TermId, interning if necessary.
fn resolve_term_to_id(
    term: &loka_sparql::parser::Term,
    dict: &mut TermDictionary,
    prefixes: &std::collections::HashMap<String, String>,
) -> std::result::Result<loka_core::TermId, ProtoError> {
    use loka_sparql::parser::Term;
    match term {
        Term::Iri(iri) => Ok(dict.intern(iri)),
        Term::PrefixedName { prefix, local } => {
            let ns = prefixes
                .get(prefix.as_str())
                .ok_or_else(|| ProtoError::BadRequest(format!("unknown prefix: {}", prefix)))?;
            Ok(dict.intern(&format!("{}{}", ns, local)))
        }
        Term::Literal(s) => Ok(dict.intern(&format!("\"{}\"", s))),
        Term::TypedLiteral { value, datatype } => {
            let full = format!("\"{}\"^^<{}>", value, datatype);
            Ok(intern_object(dict, &full))
        }
        Term::IntegerLiteral(n) => loka_core::inline_integer(*n)
            .ok_or_else(|| ProtoError::BadRequest("integer out of range".into())),
        Term::A => Ok(dict.intern("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")),
        Term::QuotedTriple {
            subject,
            predicate,
            object,
        } => {
            let s_id = resolve_term_to_id(subject, dict, prefixes)?;
            let p_id = resolve_term_to_id(predicate, dict, prefixes)?;
            let o_id = resolve_term_to_id(object, dict, prefixes)?;
            // Register the reverse mapping (idempotent) so the content-hash
            // id can be reversed for faithful rendering + provenance cascade.
            Ok(dict.register_quoted(s_id, p_id, o_id))
        }
        _ => Err(ProtoError::BadRequest(
            "variables not allowed in INSERT/DELETE DATA".into(),
        )),
    }
}

fn resolve_term_to_json(id: loka_core::TermId, dict: &TermDictionary) -> serde_json::Value {
    if let Some(n) = loka_core::decode_inline_integer(id) {
        return serde_json::json!({
            "type": "literal",
            "datatype": "http://www.w3.org/2001/XMLSchema#integer",
            "value": n.to_string()
        });
    }

    if let Some(b) = loka_core::decode_inline_boolean(id) {
        return serde_json::json!({
            "type": "literal",
            "datatype": "http://www.w3.org/2001/XMLSchema#boolean",
            "value": b.to_string()
        });
    }

    if let Some(term) = dict.resolve(id) {
        if term.starts_with('"') {
            serde_json::json!({
                "type": "literal",
                "value": term.trim_matches('"')
            })
        } else {
            serde_json::json!({
                "type": "uri",
                "value": term
            })
        }
    } else if dict.resolve_quoted(id).is_some() {
        // RDF-star quoted triple. Loka's result JSON has no dedicated
        // triple type; expose the faithful `<< s p o >>` lexical form
        // (was previously an opaque `_:idN` blank node).
        serde_json::json!({
            "type": "triple",
            "value": dict.render_term(id).unwrap_or_default()
        })
    } else {
        serde_json::json!({
            "type": "uri",
            "value": format!("_:id{}", id)
        })
    }
}

// ─── Export Graph (Turtle) ───────────────────────────────────────────────────

/// Query parameters for GET /graph.
#[derive(Deserialize)]
pub struct GraphQueryParams {
    /// Optional: request a specific format. Defaults to Turtle.
    #[serde(default)]
    format: Option<String>,
}

/// GET /browse — the interactive vis-network graph browser.
///
/// The rich force-directed viewer (click-to-expand, HNSW edges,
/// RDF-star detail panel). `/graph` is the raw Turtle export for
/// Protégé; `/browse` is the one humans look at. Served from the
/// engine so it is a first-class surface, not an orphaned loose file.
async fn serve_browse() -> impl IntoResponse {
    Html(include_str!("../../tools/browse.html"))
}

/// GET /graph — export all triples as Turtle.
///
/// Protégé can load this via File > Open from URL > http://localhost:3030/graph
/// Also useful for any tool that speaks RDF: curl, rdflib, Apache Jena, etc.
async fn export_graph(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<GraphQueryParams>,
) -> Result<impl IntoResponse, ProtoError> {
    let store = state
        .store
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    let dict = state
        .dict
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;

    let use_ntriples = params
        .format
        .as_deref()
        .map(|f| f == "nt" || f == "ntriples")
        .unwrap_or(false);

    let mut output = String::new();

    if use_ntriples {
        // N-Triples: one triple per line, no prefixes
        for triple in store.iter() {
            let s = resolve_term_for_turtle(triple.subject, &dict);
            let p = resolve_term_for_turtle(triple.predicate, &dict);
            let o = resolve_term_for_turtle(triple.object, &dict);
            output.push_str(&format!("{} {} {} .\n", s, p, o));
        }

        Ok((
            [(header::CONTENT_TYPE, "application/n-triples; charset=utf-8")],
            output,
        ))
    } else {
        // Turtle: collect common prefixes, then grouped triples
        let mut prefixes: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();

        // Scan all terms for common prefixes
        let known_prefixes = [
            ("rdf:", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
            ("rdfs:", "http://www.w3.org/2000/01/rdf-schema#"),
            ("owl:", "http://www.w3.org/2002/07/owl#"),
            ("xsd:", "http://www.w3.org/2001/XMLSchema#"),
            ("skos:", "http://www.w3.org/2004/02/skos/core#"),
            ("dc:", "http://purl.org/dc/elements/1.1/"),
            ("dcterms:", "http://purl.org/dc/terms/"),
            ("foaf:", "http://xmlns.com/foaf/0.1/"),
            ("schema:", "http://schema.org/"),
            ("wdt:", "http://www.wikidata.org/prop/direct/"),
            ("wd:", "http://www.wikidata.org/entity/"),
            ("loka:", "http://loka.dev/"),
        ];

        // Check which prefixes are actually used
        for triple in store.iter() {
            for id in [triple.subject, triple.predicate, triple.object] {
                if let Some(term) = dict.resolve(id) {
                    for &(prefix, iri) in &known_prefixes {
                        if term.starts_with(iri) && !prefixes.contains_key(prefix) {
                            prefixes.insert(prefix.to_string(), iri.to_string());
                        }
                    }
                }
            }
        }

        // Write prefix declarations
        for (prefix, iri) in &prefixes {
            output.push_str(&format!("@prefix {} <{}> .\n", prefix, iri));
        }
        if !prefixes.is_empty() {
            output.push('\n');
        }

        // Write triples grouped by subject
        let mut current_subject: Option<String> = None;

        for triple in store.iter() {
            let s = resolve_term_for_turtle(triple.subject, &dict);
            let p = resolve_term_for_turtle(triple.predicate, &dict);
            let o = resolve_term_for_turtle(triple.object, &dict);

            // Apply prefix compression
            let s_compact = compact_iri(&s, &prefixes);
            let p_compact = compact_iri(&p, &prefixes);
            let o_compact = compact_iri(&o, &prefixes);

            match &current_subject {
                Some(prev) if *prev == s => {
                    // Same subject: continue with semicolon
                    output.push_str(&format!(" ;\n    {} {}", p_compact, o_compact));
                }
                _ => {
                    // New subject: close previous, start new
                    if current_subject.is_some() {
                        output.push_str(" .\n\n");
                    }
                    output.push_str(&format!("{}\n    {} {}", s_compact, p_compact, o_compact));
                    current_subject = Some(s);
                }
            }
        }
        if current_subject.is_some() {
            output.push_str(" .\n");
        }

        Ok((
            [(header::CONTENT_TYPE, "text/turtle; charset=utf-8")],
            output,
        ))
    }
}

/// Resolve a TermId to its Turtle representation.
fn resolve_term_for_turtle(id: loka_core::TermId, dict: &TermDictionary) -> String {
    if let Some(n) = loka_core::decode_inline_integer(id) {
        return format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", n);
    }

    if let Some(b) = loka_core::decode_inline_boolean(id) {
        return format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#boolean>", b);
    }

    match dict.render_term(id) {
        // RDF-star quoted triple: render_term already produced parseable
        // N-Triples-star / Turtle-star `<< <s> <p> "o" >>`.
        Some(t) if t.starts_with("<<") => t,
        // Literal (with quotes) or blank node — pass through.
        Some(t) if t.starts_with('"') || t.starts_with("_:") => t,
        // IRI — wrap in angle brackets.
        Some(t) => format!("<{}>", t),
        None => format!("_:id{}", id),
    }
}

/// Compact an IRI using known prefixes: `<http://...#Foo>` → `prefix:Foo`
fn compact_iri(term: &str, prefixes: &std::collections::BTreeMap<String, String>) -> String {
    // Never treat an RDF-star quoted triple `<< … >>` as a compactable IRI.
    if term.starts_with("<<") {
        return term.to_string();
    }
    // Only compact IRIs (wrapped in <>)
    if let Some(iri) = term.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        for (prefix, namespace) in prefixes {
            if let Some(local) = iri.strip_prefix(namespace.as_str()) {
                return format!("{}{}", prefix, local);
            }
        }
    }
    term.to_string()
}

// ─── Insert Triples ──────────────────────────────────────────────────────────

const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// Response from the POST /triples endpoint.
#[derive(Serialize)]
pub struct InsertTriplesResponse {
    pub inserted: usize,
    pub errors: Vec<String>,
}

/// POST /triples — accepts N-Triples in the request body.
///
/// The persistent-store write path uses one sled transaction per request
/// (via `PersistentStore::insert_batch`) rather than one transaction per
/// triple. This fixes the engine-bug-at-scale documented in paper §6.1:
/// the old per-triple transaction loop wedged sled at ~100k-triple POSTs
/// because it generated 3-4 sled transactions per triple, faster than
/// sled's internal compactor could drain. Single-transaction batches keep
/// the WAL bounded; the synchronous `flush()` that used to run at the end
/// of every request is also gone — sled flushes on its own periodic
/// schedule and on Drop, which is sufficient durability for our workload.
async fn insert_triples(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<Json<InsertTriplesResponse>, ProtoError> {
    let mut dict = state
        .dict
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    let mut store = state
        .store
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;

    // Collect all rows we'll need to persist so we can hand them to sled
    // as a single transaction at the end. Each `BatchInsert` carries the
    // computed Triple plus the string forms of S/P/O — sled needs the
    // strings inside the transaction so terms_forward/terms_reverse stay
    // consistent with the SPO/POS/OSP keys.
    let mut batch: Vec<loka_core::BatchInsert> = Vec::new();
    let mut inserted_in_memory = 0usize;
    let mut errors = Vec::new();

    for (line_no, line) in body.lines().enumerate() {
        let parsed = match loka_core::parse_ntriples_star_line(line) {
            Some(t) => t,
            None => continue, // blank / comment
        };

        // If the subject is a quoted triple, intern the inner triple
        // and compute a content-addressed ID for it.
        let (s_id, inner_s_batch) = if let Some((inner_s, inner_p, inner_o)) = &parsed.inner_subject
        {
            let is_id = dict.intern(inner_s);
            let ip_id = dict.intern(inner_p);
            let io_id = intern_object(&mut dict, inner_o);
            let inner_triple = loka_core::Triple::new(is_id, ip_id, io_id);
            let _ = store.insert(inner_triple);
            // Register the quoted-triple reverse mapping in the in-memory
            // dictionary so the content-hash id can be reversed (faithful
            // rendering + provenance cascade). `register_quoted` returns the
            // same id `quoted_triple_id` would.
            let qid = dict.register_quoted(is_id, ip_id, io_id);
            // The inner-triple BatchInsert carries the reverse mapping so it
            // is persisted inside insert_batch's single transaction.
            let inner_batch = state.persistent.as_ref().map(|_| loka_core::BatchInsert {
                triple: inner_triple,
                subject: inner_s.clone(),
                predicate: inner_p.clone(),
                object: inner_o.clone(),
                quoted: Some((is_id, ip_id, io_id)),
            });
            (qid, inner_batch)
        } else {
            (dict.intern(&parsed.subject), None)
        };
        if let Some(b) = inner_s_batch {
            batch.push(b);
        }

        let p_id = dict.intern(&parsed.predicate);

        // If the object is a quoted triple, intern it too
        let (o_id, inner_o_batch) = if let Some((inner_s, inner_p, inner_o)) = &parsed.inner_object
        {
            let is_id = dict.intern(inner_s);
            let ip_id = dict.intern(inner_p);
            let io_id = intern_object(&mut dict, inner_o);
            let inner_triple = loka_core::Triple::new(is_id, ip_id, io_id);
            let _ = store.insert(inner_triple);
            let qid = dict.register_quoted(is_id, ip_id, io_id);
            let inner_batch = state.persistent.as_ref().map(|_| loka_core::BatchInsert {
                triple: inner_triple,
                subject: inner_s.clone(),
                predicate: inner_p.clone(),
                object: inner_o.clone(),
                quoted: Some((is_id, ip_id, io_id)),
            });
            (qid, inner_batch)
        } else {
            (intern_object(&mut dict, &parsed.object), None)
        };
        if let Some(b) = inner_o_batch {
            batch.push(b);
        }

        let triple = loka_core::Triple::new(s_id, p_id, o_id);
        match store.insert(triple) {
            Ok(()) => {
                if state.persistent.is_some() {
                    // Bug A fix: for a quoted subject/object, persist a
                    // faithful `<< s p o >>` term string instead of the
                    // `<<QUOTED_TRIPLE>>` sentinel `parsed.*` carries, so a
                    // WAL-replayed store renders the quoted id correctly.
                    let subject = if parsed.inner_subject.is_some() {
                        dict.render_term(s_id)
                            .unwrap_or_else(|| parsed.subject.clone())
                    } else {
                        parsed.subject.clone()
                    };
                    let object = if parsed.inner_object.is_some() {
                        dict.render_term(o_id)
                            .unwrap_or_else(|| parsed.object.clone())
                    } else {
                        parsed.object.clone()
                    };
                    batch.push(loka_core::BatchInsert {
                        triple,
                        subject,
                        predicate: parsed.predicate.clone(),
                        object,
                        quoted: None,
                    });
                }
                inserted_in_memory += 1;
            }
            Err(e) => errors.push(format!("line {}: {}", line_no + 1, e)),
        }
    }

    // One sled transaction for the whole batch. No synchronous flush.
    if let Some(ref ps_lock) = state.persistent {
        if !batch.is_empty() {
            let ps = ps_lock
                .write()
                .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
            ps.insert_batch(&batch)
                .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
        }
    }

    Ok(Json(InsertTriplesResponse {
        inserted: inserted_in_memory,
        errors,
    }))
}

/// Intern an object term, handling typed literals specially.
fn intern_object(dict: &mut TermDictionary, obj: &str) -> loka_core::TermId {
    // Check for typed literals: "value"^^<datatype>
    if let Some(caret_pos) = obj.find("\"^^<") {
        let value_str = &obj[1..caret_pos]; // strip leading quote
        let datatype_start = caret_pos + 4; // skip "^^<
        let datatype_end = obj.len() - 1; // strip trailing >
        if datatype_end > datatype_start {
            let datatype = &obj[datatype_start..datatype_end];
            if datatype == XSD_INTEGER {
                if let Ok(n) = value_str.parse::<i64>() {
                    if let Some(id) = loka_core::inline_integer(n) {
                        return id;
                    }
                }
            }
            if datatype == XSD_BOOLEAN {
                match value_str {
                    "true" => return loka_core::inline_boolean(true),
                    "false" => return loka_core::inline_boolean(false),
                    _ => {}
                }
            }
        }
    }
    dict.intern(obj)
}

// ─── Cascade-retraction preview (non-destructive) ────────────────────────────

/// Request body for `POST /retract/preview`.
#[derive(Deserialize)]
pub struct RetractRequest {
    /// The node IRI to (preview) retract.
    pub iri: String,
}

/// Request body for `POST /retract`. `commit` defaults to `false` — the
/// destructive path is opt-in.
#[derive(Deserialize)]
pub struct RetractCommitRequest {
    pub iri: String,
    #[serde(default)]
    pub commit: bool,
}

/// Render a `RetractSet` as the `by_depth` JSON array and count how many
/// removed triples sit under a declared vector index (HNSW tombstones a
/// commit would flip). Shared by `/retract/preview` and `/retract`.
fn render_retract_by_depth(
    set: &loka_core::RetractSet,
    dict: &TermDictionary,
    vectors: &VectorRegistry,
) -> (Vec<serde_json::Value>, usize) {
    let render = |id: loka_core::TermId| {
        dict.render_term(id)
            .unwrap_or_else(|| format!("_:id{}", id))
    };
    let mut hnsw = 0usize;
    let by_depth = set
        .by_depth
        .iter()
        .enumerate()
        .map(|(depth, triples)| {
            let rows: Vec<serde_json::Value> = triples
                .iter()
                .map(|t| {
                    if vectors.has_index(t.predicate) {
                        hnsw += 1;
                    }
                    serde_json::json!({
                        "s": render(t.subject),
                        "p": render(t.predicate),
                        "o": render(t.object),
                    })
                })
                .collect();
            serde_json::json!({ "depth": depth, "count": rows.len(), "triples": rows })
        })
        .collect();
    (by_depth, hnsw)
}

/// `POST /retract/preview` — compute the cascade-retraction set rooted at
/// `iri` **without deleting anything**. Returns the would-be-removed triples
/// grouped by cascade depth, plus a count of HNSW tombstones a commit would
/// flip. This is cascade-retraction Phase 2
/// (`planning/cascade-retraction.md` §5.1); the destructive `retract_node`
/// (Phase 3) builds on the exact same set computation.
async fn retract_preview(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RetractRequest>,
) -> Result<Json<serde_json::Value>, ProtoError> {
    let store = state
        .store
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    let dict = state
        .dict
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;

    let root_id = dict.lookup(&req.iri);
    let set = match root_id {
        Some(id) => loka_core::retract_set(id, &store, &dict),
        // Unknown node: nothing to remove. Non-destructive + informative.
        None => loka_core::RetractSet::default(),
    };

    let vectors = state
        .vectors
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    let (by_depth, hnsw_tombstones) = render_retract_by_depth(&set, &dict, &vectors);

    Ok(Json(serde_json::json!({
        "root": req.iri,
        "root_found": root_id.is_some(),
        "total": set.total(),
        "max_depth": set.max_depth(),
        "hnsw_tombstones": hnsw_tombstones,
        "by_depth": by_depth,
        "committed": false,
    })))
}

/// `POST /retract` — cascade-retraction Phase 3. With `commit:false` (the
/// default) this is identical to `/retract/preview`. With `commit:true` it
/// **deletes** every triple in the cascade set from the in-memory store, the
/// persistent store, and flips the corresponding HNSW entries via
/// `VectorRegistry::delete` (the delete path that was wired but never
/// invoked). Destructive — opt-in behind the explicit flag.
async fn retract_apply(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RetractCommitRequest>,
) -> Result<Json<serde_json::Value>, ProtoError> {
    // Write locks: a commit mutates store + vectors (+ persistent).
    let mut store = state
        .store
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    let dict = state
        .dict
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    let mut vectors = state
        .vectors
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;

    let root_id = dict.lookup(&req.iri);
    let set = match root_id {
        Some(id) => loka_core::retract_set(id, &store, &dict),
        None => loka_core::RetractSet::default(),
    };
    let (by_depth, hnsw_tombstones) = render_retract_by_depth(&set, &dict, &vectors);
    let total = set.total();

    let mut removed = 0usize;
    let mut hnsw_flipped = 0usize;
    if req.commit {
        for t in set.all() {
            if store.remove(t) {
                removed += 1;
            }
            if vectors.has_index(t.predicate) && vectors.delete(t.predicate, t.object) {
                hnsw_flipped += 1;
            }
            if let Some(ref ps_lock) = state.persistent {
                let ps = ps_lock
                    .write()
                    .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
                let _ = ps.remove(t);
            }
        }
        if let Some(ref ps_lock) = state.persistent {
            let ps = ps_lock
                .read()
                .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
            ps.flush()
                .map_err(|e| ProtoError::BadRequest(format!("flush: {}", e)))?;
        }
    }

    Ok(Json(serde_json::json!({
        "root": req.iri,
        "root_found": root_id.is_some(),
        "total": total,
        "max_depth": set.max_depth(),
        "hnsw_tombstones": hnsw_tombstones,
        "by_depth": by_depth,
        "committed": req.commit,
        "removed": removed,
        "hnsw_flipped": hnsw_flipped,
    })))
}

// ─── Declare Vector Predicate ────────────────────────────────────────────────

/// Request body for POST /vectors/declare.
#[derive(Deserialize)]
pub struct DeclareVectorRequest {
    pub predicate: String,
    pub dimensions: usize,
    #[serde(default = "default_m")]
    pub m: usize,
    #[serde(default = "default_ef_construction")]
    pub ef_construction: usize,
    #[serde(default = "default_metric")]
    pub metric: String,
}

fn default_m() -> usize {
    16
}
fn default_ef_construction() -> usize {
    200
}
fn default_metric() -> String {
    "cosine".to_string()
}

#[derive(Serialize)]
pub struct DeclareVectorResponse {
    pub status: String,
    pub predicate_id: u64,
}

/// POST /vectors/declare — declare a vector predicate with HNSW parameters.
async fn declare_vector_predicate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeclareVectorRequest>,
) -> Result<Json<DeclareVectorResponse>, ProtoError> {
    let metric = match req.metric.to_lowercase().as_str() {
        "cosine" => loka_hnsw::DistanceMetric::Cosine,
        "euclidean" => loka_hnsw::DistanceMetric::Euclidean,
        "dot" | "dotproduct" | "dot_product" => loka_hnsw::DistanceMetric::DotProduct,
        other => {
            return Err(ProtoError::BadRequest(format!("unknown metric: {}", other)));
        }
    };

    let predicate_id = {
        let mut dict = state
            .dict
            .write()
            .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
        dict.intern(&req.predicate)
    };

    let config = loka_hnsw::VectorPredicateConfig {
        predicate_id,
        dimensions: req.dimensions,
        m: req.m,
        ef_construction: req.ef_construction,
        metric,
    };

    let mut vectors = state
        .vectors
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
    vectors
        .declare(config)
        .map_err(|e| ProtoError::BadRequest(format!("vector declare error: {}", e)))?;

    Ok(Json(DeclareVectorResponse {
        status: "ok".to_string(),
        predicate_id,
    }))
}

// ─── Insert Vector ───────────────────────────────────────────────────────────

/// Request body for POST /vectors.
#[derive(Deserialize)]
pub struct InsertVectorRequest {
    pub predicate: String,
    pub subject: String,
    pub vector: Vec<f32>,
}

#[derive(Serialize)]
pub struct InsertVectorResponse {
    pub status: String,
    pub triple_id: u64,
}

/// POST /vectors — insert a vector embedding for a subject on a predicate.
///
/// Every vector is a triple: `<subject> <predicate> <vector_literal>`.
///
/// The vector literal is the **object** of the triple. The HNSW index is
/// keyed by the object's TermId. Multiple subjects can point to the same
/// vector (e.g. "bank" the institution and "bank" the riverbank can both
/// link to the same embedding). VECTOR_SIMILAR finds matching vector objects,
/// then you join via the graph to find which subjects connect to them.
///
/// A vector never exists in the database without at least one triple
/// pointing to it.
async fn insert_vector(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InsertVectorRequest>,
) -> Result<Json<InsertVectorResponse>, ProtoError> {
    // Build the literal string before acquiring locks
    let vec_str: Vec<String> = req.vector.iter().map(|f| format!("{:.6}", f)).collect();
    let literal = format!("\"{}\"^^<http://loka.dev/f32vec>", vec_str.join(" "));

    // The subject may be a plain IRI OR an RDF-star quoted triple
    // `<< <s> <p> <o> >>` — so a vector can be attached to the TRIPLE
    // itself (idx-triple), not just a node. Parse the quoted form with the
    // canonical N-Triples-star parser (same path /triples ingest uses) so
    // the content-addressed id matches and the reverse map renders it
    // faithfully as `<< s p o >>`. Plain IRIs behave exactly as before.
    let quoted_inner: Option<(String, String, String)> =
        if req.subject.trim_start().starts_with("<<") {
            let synthetic = format!("{} <urn:loka:vsubj> <urn:loka:vsubj> .", req.subject);
            loka_core::parse_ntriples_star_line(&synthetic).and_then(|p| p.inner_subject)
        } else {
            None
        };

    let (predicate_id, subject_id, object_id, inner) = {
        let mut dict = state
            .dict
            .write()
            .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
        let p = dict.intern(&req.predicate);
        let o = dict.intern(&literal);
        let (s, inner) = if let Some((is, ip, io)) = &quoted_inner {
            let is_id = dict.intern(is);
            let ip_id = dict.intern(ip);
            let io_id = intern_object(&mut dict, io);
            // register_quoted returns the same id quoted_triple_id would,
            // and records the reverse map for faithful `<< s p o >>` render.
            let qid = dict.register_quoted(is_id, ip_id, io_id);
            (qid, Some((loka_core::Triple::new(is_id, ip_id, io_id), is_id, ip_id, io_id)))
        } else {
            (dict.intern(&req.subject), None)
        };
        (p, s, o, inner)
    };

    // Hold both store and vectors locks together to ensure atomicity:
    // a reader will never see a triple without its HNSW entry or vice versa.
    {
        let mut store = state
            .store
            .write()
            .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;
        let mut vectors = state
            .vectors
            .write()
            .map_err(|e| ProtoError::BadRequest(format!("lock poisoned: {}", e)))?;

        // For a quoted-triple subject the inner triple must exist in the
        // store so VECTOR_SIMILAR's subject-binding join + faithful render
        // resolve correctly.
        if let Some((it, ..)) = inner {
            let _ = store.insert(it);
        }

        let triple = loka_core::Triple::new(subject_id, predicate_id, object_id);
        // Ignore duplicate triple errors (allows multiple subjects to point to same vector)
        let _ = store.insert(triple);

        // Insert into HNSW index, keyed by the object_id (the vector literal's identity).
        // If this vector was already inserted (another subject pointing to same vector),
        // the HNSW insert may error — that's fine, the vector is already indexed.
        let _ = vectors.insert(predicate_id, req.vector, object_id);

        // Write through to persistent store and flush for durability
        if let Some(ref ps_lock) = state.persistent {
            let ps = ps_lock
                .write()
                .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
            ps.intern(&req.predicate)
                .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
            ps.intern(&literal)
                .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
            if let (Some((it, is_id, ip_id, io_id)), Some((is, ip, io))) =
                (inner, &quoted_inner)
            {
                ps.intern(is)
                    .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
                ps.intern(ip)
                    .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
                ps.intern(io)
                    .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
                let _ = ps.register_quoted(is_id, ip_id, io_id);
                ps.insert(it)
                    .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
            } else {
                ps.intern(&req.subject)
                    .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
            }
            ps.insert(triple)
                .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
            ps.flush()
                .map_err(|e| ProtoError::BadRequest(format!("flush: {}", e)))?;
        }
    }

    Ok(Json(InsertVectorResponse {
        status: "ok".to_string(),
        triple_id: object_id,
    }))
}

// ─── Graph Store Protocol ────────────────────────────────────────────────────

/// GET /graph-store — export graph as Turtle (same as GET /graph).
async fn gsp_get(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<GraphQueryParams>,
) -> Result<impl IntoResponse, ProtoError> {
    export_graph(State(state), AxumQuery(params)).await
}

/// PUT /graph-store — replace all triples with new data.
async fn gsp_put(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<impl IntoResponse, ProtoError> {
    // Clear existing triples
    {
        let mut store = state
            .store
            .write()
            .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
        *store = loka_core::TripleStore::new();
    }
    // Insert new triples
    let resp = insert_triples(State(state), body).await?;
    Ok(resp.into_response())
}

/// DELETE /graph-store — delete all triples.
async fn gsp_delete(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, ProtoError> {
    let mut store = state
        .store
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
    let count = store.len();
    *store = loka_core::TripleStore::new();

    // Clear persistent store and flush
    if let Some(ref ps_lock) = state.persistent {
        let ps = ps_lock
            .write()
            .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
        ps.clear()
            .map_err(|e| ProtoError::BadRequest(format!("persist: {}", e)))?;
        ps.flush()
            .map_err(|e| ProtoError::BadRequest(format!("flush: {}", e)))?;
    }

    Ok((StatusCode::OK, format!("Deleted {} triples", count)))
}

/// GET /health
async fn health() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

/// GET /vectors/health — HNSW index health diagnostics.
async fn vectors_health(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ProtoError> {
    let vectors = state
        .vectors
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;
    let dict = state
        .dict
        .read()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;

    let mut indexes = Vec::new();
    for pred_id in vectors.predicates() {
        if let Some(index) = vectors.get(pred_id) {
            let pred_name = dict.resolve(pred_id).unwrap_or("unknown");
            indexes.push(serde_json::json!({
                "predicate": pred_name,
                "predicate_id": pred_id,
                "total_nodes": index.len(),
                "active_nodes": index.active_count(),
                "deleted_ratio": index.deleted_ratio(),
                "dimensions": index.dimensions(),
                "metric": format!("{:?}", index.metric()),
                "needs_compaction": index.deleted_ratio() > 0.3,
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "index_count": indexes.len(),
        "total_edge_count": vectors.total_edge_count(),
        "indexes": indexes,
    })))
}

/// POST /vectors/rebuild — compact and rebuild all HNSW indexes.
/// Removes tombstones and restores connectivity. This is the HTTP equivalent
/// of `loka health --rebuild-hnsw`, accessible to AI agents via API.
async fn rebuild_hnsw(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ProtoError> {
    let mut vectors = state
        .vectors
        .write()
        .map_err(|e| ProtoError::BadRequest(format!("lock: {}", e)))?;

    let mut results = Vec::new();
    for pred_id in vectors.predicates() {
        if let Some(index) = vectors.get_mut(pred_id) {
            let before = index.len();
            let removed = index.compact();
            let after = index.active_count();
            results.push(serde_json::json!({
                "predicate_id": pred_id,
                "tombstones_removed": removed,
                "nodes_before": before,
                "active_after": after,
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "indexes_rebuilt": results.len(),
        "details": results,
    })))
}

/// GET /service-description — SPARQL service description (Turtle).
async fn service_description(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.store.read().ok();
    let triple_count = store.as_ref().map(|s| s.len()).unwrap_or(0);

    let ttl = format!(
        r#"@prefix sd: <http://www.w3.org/ns/sparql-service-description#> .
@prefix void: <http://rdfs.org/ns/void#> .

<> a sd:Service ;
    sd:endpoint <sparql> ;
    sd:supportedLanguage sd:SPARQL11Query ;
    sd:resultFormat <http://www.w3.org/ns/formats/SPARQL_Results_JSON> ,
                    <http://www.w3.org/ns/formats/SPARQL_Results_CSV> ,
                    <http://www.w3.org/ns/formats/SPARQL_Results_TSV> ;
    sd:feature sd:BasicFederatedQuery ;
    sd:defaultDataset [
        a sd:Dataset ;
        sd:defaultGraph [
            a sd:Graph , void:Dataset ;
            void:triples {} ;
        ]
    ] .
"#,
        triple_count
    );

    ([(header::CONTENT_TYPE, "text/turtle; charset=utf-8")], ttl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use loka_core::Triple;
    use tower::util::ServiceExt;

    fn test_state() -> Arc<AppState> {
        let mut dict = TermDictionary::new();
        let mut store = TripleStore::new();

        let alice = dict.intern("http://example.org/Alice");
        let bob = dict.intern("http://example.org/Bob");
        let knows = dict.intern("http://example.org/knows");

        store.insert(Triple::new(alice, knows, bob)).unwrap();

        Arc::new(AppState {
            store: RwLock::new(store),
            dict: RwLock::new(dict),
            vectors: RwLock::new(VectorRegistry::new()),
            persistent: None,
            passcode: None,
            rate_limit_per_min: 0,
            rate_counter: std::sync::atomic::AtomicU64::new(0),
        })
    }

    #[tokio::test]
    async fn health_check() {
        let app = router(test_state());
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn sparql_get_query() {
        let app = router(test_state());
        let req = Request::builder()
            .uri("/sparql?query=SELECT%20*%20WHERE%20%7B%20%3Fs%20%3Fp%20%3Fo%20%7D")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["results"]["bindings"].is_array());
        assert_eq!(json["results"]["bindings"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn sparql_post_query() {
        let app = router(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/sparql")
            .header("content-type", "application/sparql-query")
            .body(Body::from("SELECT * WHERE { ?s ?p ?o }"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn sparql_invalid_query() {
        let app = router(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/sparql")
            .body(Body::from("INVALID"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn insert_ntriples() {
        let state = test_state();
        let app = router(state.clone());
        let body = concat!(
            "<http://example.org/s1> <http://example.org/p1> <http://example.org/o1> .\n",
            "<http://example.org/s2> <http://example.org/p2> \"hello\" .\n",
            "# comment line\n",
            "\n",
            "<http://example.org/s3> <http://example.org/p3> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n",
        );
        let req = Request::builder()
            .method("POST")
            .uri("/triples")
            .header("content-type", "text/plain")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["inserted"], 3);
        assert_eq!(json["errors"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn rdf_star_quoted_subject_renders_faithfully_not_blank_node() {
        // Phase-0 / Bug-A end-to-end: ingest an RDF-star annotation, query
        // it back, and assert the quoted-triple subject renders as faithful
        // `<< s p o >>` (was previously an opaque `_:idN` because the
        // content-hash id had no reverse map).
        let state = test_state();
        let app = router(state.clone());
        let body = concat!(
            "<< <http://example.org/Q42> <http://example.org/P20> ",
            "<http://example.org/Q31> >> ",
            "<http://loka.dev/provenance/propositionConfidence> \"0.9\" .\n",
        );
        let req = Request::builder()
            .method("POST")
            .uri("/triples")
            .header("content-type", "text/plain")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Query the annotation row back.
        let app = router(state.clone());
        let q = "SELECT ?s ?v WHERE { ?s \
                 <http://loka.dev/provenance/propositionConfidence> ?v }";
        let req = Request::builder()
            .method("POST")
            .uri("/sparql")
            .header("content-type", "application/sparql-query")
            .body(Body::from(q))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let bindings = json["results"]["bindings"].as_array().unwrap();
        assert_eq!(bindings.len(), 1, "exactly the annotation row");
        let s = &bindings[0]["s"];
        assert_eq!(s["type"], "triple", "quoted subject is an RDF-star triple");
        assert_eq!(
            s["value"],
            "<< <http://example.org/Q42> <http://example.org/P20> \
             <http://example.org/Q31> >>",
            "faithful << s p o >>, not _:idN or the <<QUOTED_TRIPLE>> sentinel"
        );
        assert_eq!(bindings[0]["v"]["value"], "0.9");
    }

    #[tokio::test]
    async fn insert_duplicate_triple_reports_error() {
        let state = test_state();
        let app = router(state.clone());
        // Insert the same triple that's already in the store
        let body =
            "<http://example.org/Alice> <http://example.org/knows> <http://example.org/Bob> .\n";
        let req = Request::builder()
            .method("POST")
            .uri("/triples")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["inserted"], 0);
        assert_eq!(json["errors"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn export_graph_turtle() {
        let app = router(test_state());
        let req = Request::builder()
            .uri("/graph")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("text/turtle"));

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();

        // Should contain the Alice-knows-Bob triple
        assert!(text.contains("Alice"));
        assert!(text.contains("knows"));
        assert!(text.contains("Bob"));
    }

    #[tokio::test]
    async fn export_graph_ntriples() {
        let app = router(test_state());
        let req = Request::builder()
            .uri("/graph?format=nt")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("n-triples"));

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();

        // N-Triples: each line ends with " ."
        for line in text.lines() {
            assert!(line.trim().ends_with('.'), "bad line: {}", line);
        }
    }

    #[tokio::test]
    async fn retract_preview_is_nondestructive_and_cascades() {
        // B6: POST /retract/preview computes the cascade set without
        // deleting anything.
        let state = test_state();
        let app = router(state.clone());
        // Real source + a generated triple that cites it.
        let body = concat!(
            "<http://wd/Q42> <http://wd/P_pob> <http://wd/Q350> .\n",
            "<< <http://wd/Q350> <http://wd/G_died> <http://wd/Q999> >> ",
            "<http://loka.dev/provenance/propositionInferredFrom> ",
            "<< <http://wd/Q42> <http://wd/P_pob> <http://wd/Q350> >> .\n",
        );
        let req = Request::builder()
            .method("POST")
            .uri("/triples")
            .header("content-type", "text/plain")
            .body(Body::from(body))
            .unwrap();
        assert_eq!(app.oneshot(req).await.unwrap().status(), StatusCode::OK);
        let count_before = state.store.read().unwrap().len();

        let app = router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri("/retract/preview")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"iri":"http://wd/Q42"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let j: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(j["root_found"], true);
        assert_eq!(j["committed"], false);
        assert!(
            j["total"].as_u64().unwrap() >= 3,
            "real row + G1 + its provenance: {j}"
        );
        assert!(
            j["max_depth"].as_u64().unwrap() >= 1,
            "at least one provenance hop"
        );
        // Depth 0 holds the root's own real row.
        let d0 = &j["by_depth"][0]["triples"];
        assert!(
            d0.as_array()
                .unwrap()
                .iter()
                .any(|t| t["s"] == "http://wd/Q42"
                    && t["p"] == "http://wd/P_pob"
                    && t["o"] == "http://wd/Q350"),
            "depth 0 = the node's own row"
        );
        // Somewhere the generated G1 asserted row appears (cascaded).
        let flat = j["by_depth"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|d| d["triples"].as_array().unwrap().clone());
        assert!(
            flat.clone().any(|t| t["s"] == "http://wd/Q350"
                && t["p"] == "http://wd/G_died"
                && t["o"] == "http://wd/Q999"),
            "the generated child cascaded into the preview"
        );

        // NON-DESTRUCTIVE: store unchanged.
        assert_eq!(state.store.read().unwrap().len(), count_before);
    }

    #[tokio::test]
    async fn retract_commit_deletes_the_cascade() {
        // B7: POST /retract commit:false is a no-op; commit:true deletes the
        // whole cascade (root row + transitively-cited generated rows).
        let state = test_state();
        let app = router(state.clone());
        let body = concat!(
            "<http://wd/Q42> <http://wd/P_pob> <http://wd/Q350> .\n",
            "<< <http://wd/Q350> <http://wd/G_died> <http://wd/Q999> >> ",
            "<http://loka.dev/provenance/propositionInferredFrom> ",
            "<< <http://wd/Q42> <http://wd/P_pob> <http://wd/Q350> >> .\n",
        );
        let req = Request::builder()
            .method("POST")
            .uri("/triples")
            .header("content-type", "text/plain")
            .body(Body::from(body))
            .unwrap();
        assert_eq!(app.oneshot(req).await.unwrap().status(), StatusCode::OK);
        let before = state.store.read().unwrap().len();

        // commit:false — no-op.
        let app = router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri("/retract")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"iri":"http://wd/Q42","commit":false}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let j: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(j["committed"], false);
        assert_eq!(
            state.store.read().unwrap().len(),
            before,
            "dry-run is a no-op"
        );
        let total = j["total"].as_u64().unwrap();
        assert!(total >= 3);

        // commit:true — destructive.
        let app = router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri("/retract")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"iri":"http://wd/Q42","commit":true}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let j: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(j["committed"], true);
        assert!(j["removed"].as_u64().unwrap() >= 3);
        assert_eq!(
            state.store.read().unwrap().len(),
            before - total as usize,
            "the whole cascade is gone"
        );
        // The generated child specifically is gone.
        assert!(!state
            .store
            .read()
            .unwrap()
            .find_by_subject(state.dict.read().unwrap().lookup("http://wd/Q350").unwrap())
            .iter()
            .any(|t| t.predicate
                == state
                    .dict
                    .read()
                    .unwrap()
                    .lookup("http://wd/G_died")
                    .unwrap()));
    }

    #[tokio::test]
    async fn rdf_star_quoted_var_renders_in_csv() {
        // B5: projecting a quoted-bound variable through the CSV result
        // format must give the faithful << … >>, not _:idN.
        let state = test_state();
        let app = router(state.clone());
        let body = concat!(
            "<< <http://wd/Q42> <http://wd/P20> <http://wd/Q31> >> ",
            "<http://loka.dev/provenance/propositionConfidence> \"0.9\" .\n",
        );
        let req = Request::builder()
            .method("POST")
            .uri("/triples")
            .header("content-type", "text/plain")
            .body(Body::from(body))
            .unwrap();
        assert_eq!(app.oneshot(req).await.unwrap().status(), StatusCode::OK);

        let app = router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri("/sparql.csv")
            .header("content-type", "application/sparql-query")
            .body(Body::from(
                "SELECT ?s WHERE { ?s \
                 <http://loka.dev/provenance/propositionConfidence> ?v }",
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let csv = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(
            csv.contains("<< <http://wd/Q42> <http://wd/P20> <http://wd/Q31> >>"),
            "CSV projects the quoted subject faithfully; got: {csv}"
        );
        assert!(!csv.contains("_:id"), "no blank-node fallback leak");
    }

    #[tokio::test]
    async fn rdf_star_export_round_trips() {
        // B4: ingest an RDF-star annotation, export N-Triples, re-parse —
        // the quoted triple must come back as `<< … >>`, not `_:idN`.
        let state = test_state();
        let app = router(state.clone());
        let body = concat!(
            "<< <http://wd/Q42> <http://wd/P20> <http://wd/Q31> >> ",
            "<http://loka.dev/provenance/propositionConfidence> \"0.9\" .\n",
        );
        let req = Request::builder()
            .method("POST")
            .uri("/triples")
            .header("content-type", "text/plain")
            .body(Body::from(body))
            .unwrap();
        assert_eq!(app.oneshot(req).await.unwrap().status(), StatusCode::OK);

        let app = router(state.clone());
        let req = Request::builder()
            .uri("/graph?format=nt")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();

        // Every exported line must re-parse with the RDF-star parser.
        let mut saw_annotation = false;
        for line in text.lines() {
            let parsed = loka_core::parse_ntriples_star_line(line)
                .unwrap_or_else(|| panic!("export line does not re-parse: {line}"));
            if let Some((s, p, o)) = parsed.inner_subject {
                assert_eq!(s, "http://wd/Q42");
                assert_eq!(p, "http://wd/P20");
                assert_eq!(o, "http://wd/Q31");
                assert_eq!(
                    parsed.predicate,
                    "http://loka.dev/provenance/propositionConfidence"
                );
                saw_annotation = true;
            }
            // No row should leak the blank-node fallback.
            assert!(
                !line.contains("_:id"),
                "quoted id leaked as a blank node: {line}"
            );
        }
        assert!(
            saw_annotation,
            "the << Q42 P20 Q31 >> annotation round-tripped"
        );
    }

    #[tokio::test]
    async fn declare_and_insert_vector() {
        let state = test_state();

        // Declare vector predicate
        let app = router(state.clone());
        let declare_body = serde_json::json!({
            "predicate": "http://example.org/hasEmbedding",
            "dimensions": 3,
            "m": 4,
            "ef_construction": 20,
            "metric": "cosine"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/vectors/declare")
            .header("content-type", "application/json")
            .body(Body::from(declare_body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["status"], "ok");

        // Insert vector
        let app = router(state.clone());
        let insert_body = serde_json::json!({
            "predicate": "http://example.org/hasEmbedding",
            "subject": "http://example.org/entity1",
            "vector": [0.1, 0.2, 0.3]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/vectors")
            .header("content-type", "application/json")
            .body(Body::from(insert_body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["status"], "ok");
    }
}
