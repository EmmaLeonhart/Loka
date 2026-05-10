//! Loka Playground Server
//!
//! Starts a pre-loaded Loka instance with a Shinto shrine knowledge graph
//! including vector embeddings and temporal annotations. Serves both the
//! SPARQL endpoint and the interactive HTML playground.
//!
//! Run: cargo run --example playground_server -p loka-proto
//! Then open: http://localhost:3030

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use loka_core::{
    inline_integer, quoted_triple_id, TemporalSignifier, TermDictionary, TermId, Triple,
    TripleStore,
};
use loka_hnsw::{DistanceMetric, VectorPredicateConfig, VectorRegistry};
use loka_proto::{router, AppState};

/// Build the demo dataset: 5 shrines, 4 deities, 2 priests, 2 myths,
/// with vector embeddings and temporal annotations.
fn build_demo() -> (TripleStore, TermDictionary, VectorRegistry) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let mut vectors = VectorRegistry::new();

    let rdf_type = dict.intern("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");

    // Classes
    let shrine_class = dict.intern("http://example.org/Shrine");
    let deity_class = dict.intern("http://example.org/Deity");
    let person_class = dict.intern("http://example.org/Person");
    let myth_class = dict.intern("http://example.org/Myth");

    // Predicates
    let name = dict.intern("http://example.org/name");
    let founded = dict.intern("http://example.org/foundedYear");
    let enshrines = dict.intern("http://example.org/enshrines");
    let domain = dict.intern("http://example.org/domain");
    let located_in = dict.intern("http://example.org/locatedIn");
    let rank = dict.intern("http://example.org/rank");
    let appears_in = dict.intern("http://example.org/appearsIn");
    let alt_name = dict.intern("http://example.org/alternateName");
    let confidence = dict.intern("http://example.org/confidence");
    let source = dict.intern("http://example.org/source");
    let has_embedding = dict.intern("http://example.org/hasEmbedding");
    let role = dict.intern("http://example.org/role");
    let serves_at = dict.intern("http://example.org/servesAt");

    // Shrines
    let ise = dict.intern("http://example.org/IseJingu");
    let izumo = dict.intern("http://example.org/IzumoTaisha");
    let fushimi = dict.intern("http://example.org/FushimiInari");
    let meiji = dict.intern("http://example.org/MeijiJingu");
    let kasuga = dict.intern("http://example.org/KasugaTaisha");

    // Deities
    let amaterasu = dict.intern("http://example.org/Amaterasu");
    let okuninushi = dict.intern("http://example.org/Okuninushi");
    let inari = dict.intern("http://example.org/Inari");
    let emperor_meiji = dict.intern("http://example.org/EmperorMeiji");
    let takemikazuchi = dict.intern("http://example.org/Takemikazuchi");

    // Locations
    let mie = dict.intern("http://example.org/Mie");
    let shimane = dict.intern("http://example.org/Shimane");
    let kyoto = dict.intern("http://example.org/Kyoto");
    let tokyo = dict.intern("http://example.org/Tokyo");
    let nara = dict.intern("http://example.org/Nara");
    let osaka = dict.intern("http://example.org/Osaka");

    // Domains
    let sun = dict.intern("http://example.org/Sun");
    let earth = dict.intern("http://example.org/Earth");
    let harvest = dict.intern("http://example.org/Harvest");
    let thunder = dict.intern("http://example.org/Thunder");

    // Myths
    let kojiki = dict.intern("http://example.org/Kojiki");
    let nihon_shoki = dict.intern("http://example.org/NihonShoki");

    // People
    let tanaka = dict.intern("http://example.org/Tanaka");
    let suzuki = dict.intern("http://example.org/Suzuki");

    // Roles
    let chief_priest = dict.intern("http://example.org/ChiefPriest");
    let kannushi = dict.intern("http://example.org/Kannushi");

    // ── Literal names ──
    let n = |d: &mut TermDictionary, s: &str| d.intern(&format!("\"{s}\""));
    let n_ise = n(&mut dict, "Ise Jingu");
    let n_izumo = n(&mut dict, "Izumo Taisha");
    let n_fushimi = n(&mut dict, "Fushimi Inari Taisha");
    let n_meiji = n(&mut dict, "Meiji Jingu");
    let n_kasuga = n(&mut dict, "Kasuga Taisha");
    let n_amaterasu = n(&mut dict, "Amaterasu");
    let n_okuninushi = n(&mut dict, "Okuninushi");
    let n_inari = n(&mut dict, "Inari");
    let n_takemikazuchi = n(&mut dict, "Takemikazuchi");
    let n_kojiki = n(&mut dict, "Kojiki");
    let n_nihon = n(&mut dict, "Nihon Shoki");
    let n_tanaka = n(&mut dict, "Tanaka Haruki");
    let n_suzuki = n(&mut dict, "Suzuki Yuki");

    // ── Types ──
    for &s in &[ise, izumo, fushimi, meiji, kasuga] {
        store
            .insert(Triple::new(s, rdf_type, shrine_class))
            .unwrap();
    }
    for &d in &[amaterasu, okuninushi, inari, takemikazuchi] {
        store.insert(Triple::new(d, rdf_type, deity_class)).unwrap();
    }
    for &p in &[tanaka, suzuki] {
        store
            .insert(Triple::new(p, rdf_type, person_class))
            .unwrap();
    }
    for &m in &[kojiki, nihon_shoki] {
        store.insert(Triple::new(m, rdf_type, myth_class)).unwrap();
    }

    // ── Names ──
    let name_pairs: &[(TermId, TermId)] = &[
        (ise, n_ise),
        (izumo, n_izumo),
        (fushimi, n_fushimi),
        (meiji, n_meiji),
        (kasuga, n_kasuga),
        (amaterasu, n_amaterasu),
        (okuninushi, n_okuninushi),
        (inari, n_inari),
        (takemikazuchi, n_takemikazuchi),
        (kojiki, n_kojiki),
        (nihon_shoki, n_nihon),
        (tanaka, n_tanaka),
        (suzuki, n_suzuki),
    ];
    for &(s, o) in name_pairs {
        store.insert(Triple::new(s, name, o)).unwrap();
    }

    // ── Alternate names ──
    store
        .insert(Triple::new(ise, alt_name, n(&mut dict, "The Grand Shrine")))
        .unwrap();
    store
        .insert(Triple::new(fushimi, alt_name, n(&mut dict, "O-Inari-san")))
        .unwrap();

    // ── Founded years ──
    let years: &[(TermId, i64)] = &[
        (ise, -4),
        (izumo, 659),
        (fushimi, 711),
        (meiji, 1920),
        (kasuga, 768),
    ];
    for &(s, y) in years {
        store
            .insert(Triple::new(s, founded, inline_integer(y).unwrap()))
            .unwrap();
    }

    // ── Ranks ──
    for (i, &s) in [ise, izumo, fushimi, meiji, kasuga].iter().enumerate() {
        store
            .insert(Triple::new(s, rank, inline_integer(i as i64 + 1).unwrap()))
            .unwrap();
    }

    // ── Locations ──
    let locs: &[(TermId, TermId)] = &[
        (ise, mie),
        (izumo, shimane),
        (fushimi, kyoto),
        (meiji, tokyo),
        (kasuga, nara),
    ];
    for &(s, o) in locs {
        store.insert(Triple::new(s, located_in, o)).unwrap();
    }

    // ── Enshrinement ──
    store
        .insert(Triple::new(ise, enshrines, amaterasu))
        .unwrap();
    store
        .insert(Triple::new(izumo, enshrines, okuninushi))
        .unwrap();
    store
        .insert(Triple::new(fushimi, enshrines, inari))
        .unwrap();
    store
        .insert(Triple::new(meiji, enshrines, emperor_meiji))
        .unwrap();
    store
        .insert(Triple::new(kasuga, enshrines, takemikazuchi))
        .unwrap();
    store
        .insert(Triple::new(kasuga, enshrines, amaterasu))
        .unwrap();

    // ── Deity domains ──
    store.insert(Triple::new(amaterasu, domain, sun)).unwrap();
    store
        .insert(Triple::new(okuninushi, domain, earth))
        .unwrap();
    store.insert(Triple::new(inari, domain, harvest)).unwrap();
    store
        .insert(Triple::new(takemikazuchi, domain, thunder))
        .unwrap();

    // ── Myths ──
    store
        .insert(Triple::new(amaterasu, appears_in, kojiki))
        .unwrap();
    store
        .insert(Triple::new(amaterasu, appears_in, nihon_shoki))
        .unwrap();
    store
        .insert(Triple::new(okuninushi, appears_in, kojiki))
        .unwrap();
    store
        .insert(Triple::new(takemikazuchi, appears_in, kojiki))
        .unwrap();
    store
        .insert(Triple::new(takemikazuchi, appears_in, nihon_shoki))
        .unwrap();

    // ── People ──
    store
        .insert(Triple::new(tanaka, role, chief_priest))
        .unwrap();
    store.insert(Triple::new(suzuki, role, kannushi)).unwrap();

    // ── RDF-star: confidence + provenance on edges ──
    let qt_ise = quoted_triple_id(ise, enshrines, amaterasu);
    store
        .insert(Triple::new(qt_ise, confidence, n(&mut dict, "0.99")))
        .unwrap();
    store
        .insert(Triple::new(
            qt_ise,
            source,
            n(&mut dict, "academic_survey_2023"),
        ))
        .unwrap();

    let qt_izumo = quoted_triple_id(izumo, enshrines, okuninushi);
    store
        .insert(Triple::new(qt_izumo, confidence, n(&mut dict, "0.95")))
        .unwrap();
    store
        .insert(Triple::new(
            qt_izumo,
            source,
            n(&mut dict, "Kojiki_text_analysis"),
        ))
        .unwrap();

    let qt_kasuga = quoted_triple_id(kasuga, enshrines, amaterasu);
    store
        .insert(Triple::new(qt_kasuga, confidence, n(&mut dict, "0.70")))
        .unwrap();

    // ── Vector embeddings (4D) ──
    vectors
        .declare(VectorPredicateConfig {
            predicate_id: has_embedding,
            dimensions: 4,
            m: 4,
            ef_construction: 20,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    let vecs: &[(TermId, [f32; 4])] = &[
        (ise, [0.9, 0.3, 0.1, 0.0]),
        (izumo, [0.7, 0.5, 0.2, 0.1]),
        (fushimi, [0.6, 0.4, 0.8, 0.1]),
        (meiji, [0.1, 0.1, 0.1, 0.9]),
        (kasuga, [0.8, 0.4, 0.2, 0.0]),
    ];
    for &(entity, ref vec) in vecs {
        let label = format!(
            "\"vec_{}\"^^<http://loka.dev/f32vec>",
            dict.resolve(entity).unwrap()
        );
        let vec_id = dict.intern(&label);
        store
            .insert(Triple::new(entity, has_embedding, vec_id))
            .unwrap();
        vectors.insert(has_embedding, vec.to_vec(), vec_id).unwrap();
    }

    // ── Temporal annotations ──
    // Tanaka served at Ise 1900-1950, Suzuki from 1950 onward
    store.insert(Triple::new(tanaka, serves_at, ise)).unwrap();
    store.insert(Triple::new(suzuki, serves_at, ise)).unwrap();
    store.insert_temporal(TemporalSignifier::ValidFrom, 1900, tanaka, serves_at, ise);
    store.insert_temporal(TemporalSignifier::ValidTo, 1950, tanaka, serves_at, ise);
    store.insert_temporal(TemporalSignifier::ValidFrom, 1950, suzuki, serves_at, ise);

    // Fushimi temporarily in Osaka during Onin War (1467-1499)
    store
        .insert(Triple::new(fushimi, located_in, osaka))
        .unwrap();
    store.insert_temporal(
        TemporalSignifier::ValidFrom,
        1467,
        fushimi,
        located_in,
        osaka,
    );
    store.insert_temporal(TemporalSignifier::ValidTo, 1499, fushimi, located_in, osaka);
    store.insert_temporal(
        TemporalSignifier::ValidFrom,
        711,
        fushimi,
        located_in,
        kyoto,
    );
    store.insert_temporal(TemporalSignifier::ValidFrom, 1920, meiji, located_in, tokyo);

    // Pre-intern change type literals for TEMPORAL_DIFF
    dict.intern("\"added\"");
    dict.intern("\"removed\"");
    dict.intern("\"unchanged\"");

    (store, dict, vectors)
}

const PLAYGROUND_HTML: &str = include_str!("../../pages/playground.html");

async fn serve_playground() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        PLAYGROUND_HTML,
    )
}

#[tokio::main]
async fn main() {
    let (store, dict, vectors) = build_demo();
    let triple_count = store.len();

    let state = Arc::new(AppState {
        store: RwLock::new(store),
        dict: RwLock::new(dict),
        vectors: RwLock::new(vectors),
        persistent: None,
        passcode: None,
        rate_limit_per_min: 0,
        rate_counter: AtomicU64::new(0),
    });

    // Combine the SPARQL API router with the playground page
    let app = Router::new()
        .route("/", get(serve_playground))
        .merge(router(state));

    let port = std::env::var("LOKA_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3030);
    let addr = format!("0.0.0.0:{port}");
    let url = format!("http://localhost:{port}");

    println!();
    println!("  Loka Playground");
    println!("  {triple_count} triples loaded (shrines, deities, myths, vectors, temporal)");
    println!();
    println!("  Playground:  {url}");
    println!("  SPARQL:      {url}/sparql");
    println!("  Graph Store: {url}/graph-store");
    println!();
    println!("  Press Ctrl+C to stop.");
    println!();

    // Auto-open browser
    let _ = open_browser(&url);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}
