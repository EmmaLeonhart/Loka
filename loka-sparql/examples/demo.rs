//! Loka Feature Demo
//!
//! A comprehensive demonstration of every major query feature:
//!   1. Basic triple patterns & joins
//!   2. FILTER (numeric, string, boolean)
//!   3. OPTIONAL (left join)
//!   4. UNION
//!   5. Aggregates (COUNT, GROUP BY)
//!   6. ORDER BY / LIMIT / OFFSET / DISTINCT
//!   7. BIND / VALUES
//!   8. RDF-star (statements about statements)
//!   9. Vector search (VECTOR_SIMILAR / VECTOR_SCORE)
//!  10. Temporal operators (AT_TIME, DURING, WORLD_STATE, TEMPORAL_DIFF)
//!
//! Run: cargo run --example demo -p loka-sparql

use loka_core::{
    inline_integer, quoted_triple_id, TemporalSignifier, TermDictionary, TermId, Triple,
    TripleStore,
};
use loka_hnsw::{DistanceMetric, VectorPredicateConfig, VectorRegistry};
use loka_sparql::{execute_with_vectors, parse, QueryResult};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn resolve(dict: &TermDictionary, id: TermId) -> String {
    if loka_core::is_inline(id) {
        if let Some(n) = loka_core::decode_inline_integer(id) {
            return n.to_string();
        }
        if let Some(b) = loka_core::decode_inline_boolean(id) {
            return b.to_string();
        }
        return format!("inline:{id}");
    }
    dict.resolve(id).unwrap_or(&format!("?{id}")).to_string()
}

fn run_query(
    label: &str,
    sparql: &str,
    store: &TripleStore,
    dict: &TermDictionary,
    vectors: &VectorRegistry,
) {
    println!("\n{}", "=".repeat(72));
    println!("  {label}");
    println!("{}", "=".repeat(72));
    println!("{sparql}\n");

    let query = match parse(sparql) {
        Ok(q) => q,
        Err(e) => {
            println!("  PARSE ERROR: {e}");
            return;
        }
    };
    match execute_with_vectors(&query, store, dict, vectors) {
        Ok(result) => print_result(&result, dict, store),
        Err(e) => println!("  EXEC ERROR: {e}"),
    }
}

fn print_result(result: &QueryResult, dict: &TermDictionary, _store: &TripleStore) {
    if result.columns.is_empty() {
        println!("  (no columns)");
        return;
    }

    // Column widths
    let mut widths: Vec<usize> = result.columns.iter().map(|c| c.len() + 1).collect();
    let resolved_rows: Vec<Vec<String>> = result
        .rows
        .iter()
        .map(|row| {
            result
                .columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let s = row
                        .get(col)
                        .map(|id| resolve(dict, *id))
                        .unwrap_or_else(|| "(unbound)".into());
                    if s.len() + 1 > widths[i] {
                        widths[i] = s.len() + 1;
                    }
                    s
                })
                .collect()
        })
        .collect();

    // Header
    let header: String = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("?{c:<w$}", w = widths[i]))
        .collect::<Vec<_>>()
        .join(" | ");
    println!("  {header}");
    let sep: String = widths
        .iter()
        .map(|w| "-".repeat(w + 1))
        .collect::<Vec<_>>()
        .join("-+-");
    println!("  {sep}");

    // Rows
    if resolved_rows.is_empty() {
        println!("  (0 rows)");
    }
    for (ri, row) in resolved_rows.iter().enumerate() {
        let line: String = row
            .iter()
            .enumerate()
            .map(|(i, v)| format!(" {v:<w$}", w = widths[i]))
            .collect::<Vec<_>>()
            .join(" |");
        // Append score if present
        let score_info = if !result.scores.is_empty() && !result.scores[ri].is_empty() {
            let scores: Vec<String> = result.scores[ri]
                .iter()
                .map(|(k, v)| format!("{k}={v:.4}"))
                .collect();
            format!("  [{}]", scores.join(", "))
        } else {
            String::new()
        };
        println!("  {line}{score_info}");
    }
    println!(
        "  ({} row{})",
        result.rows.len(),
        if result.rows.len() == 1 { "" } else { "s" }
    );
}

// ── Dataset Builder ──────────────────────────────────────────────────────────

struct DemoDb {
    store: TripleStore,
    dict: TermDictionary,
    vectors: VectorRegistry,
}

fn build_demo_db() -> DemoDb {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let mut vectors = VectorRegistry::new();

    // ── Prefixes we'll use in queries ──
    let rdf_type = dict.intern("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");

    // ── Classes ──
    let shrine_class = dict.intern("http://example.org/Shrine");
    let deity_class = dict.intern("http://example.org/Deity");
    let person_class = dict.intern("http://example.org/Person");
    let myth_class = dict.intern("http://example.org/Myth");

    // ── Predicates ──
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

    // ── Entities: Shrines ──
    let ise = dict.intern("http://example.org/IseJingu");
    let izumo = dict.intern("http://example.org/IzumoTaisha");
    let fushimi = dict.intern("http://example.org/FushimiInari");
    let meiji = dict.intern("http://example.org/MeijiJingu");
    let kasuga = dict.intern("http://example.org/KasugaTaisha");

    // ── Entities: Deities ──
    let amaterasu = dict.intern("http://example.org/Amaterasu");
    let okuninushi = dict.intern("http://example.org/Okuninushi");
    let inari = dict.intern("http://example.org/Inari");
    let emperor_meiji = dict.intern("http://example.org/EmperorMeiji");
    let takemikazuchi = dict.intern("http://example.org/Takemikazuchi");

    // ── Entities: Other ──
    let mie = dict.intern("http://example.org/Mie");
    let shimane = dict.intern("http://example.org/Shimane");
    let kyoto = dict.intern("http://example.org/Kyoto");
    let tokyo = dict.intern("http://example.org/Tokyo");
    let nara = dict.intern("http://example.org/Nara");

    let sun = dict.intern("http://example.org/Sun");
    let earth = dict.intern("http://example.org/Earth");
    let harvest = dict.intern("http://example.org/Harvest");
    let thunder = dict.intern("http://example.org/Thunder");

    let kojiki = dict.intern("http://example.org/Kojiki");
    let nihon_shoki = dict.intern("http://example.org/NihonShoki");

    let chief_priest = dict.intern("http://example.org/ChiefPriest");
    let kannushi = dict.intern("http://example.org/Kannushi");

    let tanaka = dict.intern("http://example.org/Tanaka");
    let suzuki = dict.intern("http://example.org/Suzuki");

    // ── Literal names ──
    let n_ise = dict.intern("\"Ise Jingu\"");
    let n_izumo = dict.intern("\"Izumo Taisha\"");
    let n_fushimi = dict.intern("\"Fushimi Inari Taisha\"");
    let n_meiji = dict.intern("\"Meiji Jingu\"");
    let n_kasuga = dict.intern("\"Kasuga Taisha\"");
    let n_amaterasu = dict.intern("\"Amaterasu\"");
    let n_okuninushi = dict.intern("\"Okuninushi\"");
    let n_inari = dict.intern("\"Inari\"");
    let n_takemikazuchi = dict.intern("\"Takemikazuchi\"");
    let n_kojiki = dict.intern("\"Kojiki\"");
    let n_nihon = dict.intern("\"Nihon Shoki\"");
    let n_tanaka = dict.intern("\"Tanaka Haruki\"");
    let n_suzuki = dict.intern("\"Suzuki Yuki\"");

    // Alternate names
    let alt_ise = dict.intern("\"The Grand Shrine\"");
    let alt_fushimi = dict.intern("\"O-Inari-san\"");

    // Source literals
    let src_academic = dict.intern("\"academic_survey_2023\"");
    let src_kojiki_text = dict.intern("\"Kojiki_text_analysis\"");

    // Confidence literals
    let conf_99 = dict.intern("\"0.99\"");
    let conf_95 = dict.intern("\"0.95\"");
    let conf_70 = dict.intern("\"0.70\"");

    // ── Type assertions ──
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
    store.insert(Triple::new(ise, name, n_ise)).unwrap();
    store.insert(Triple::new(izumo, name, n_izumo)).unwrap();
    store.insert(Triple::new(fushimi, name, n_fushimi)).unwrap();
    store.insert(Triple::new(meiji, name, n_meiji)).unwrap();
    store.insert(Triple::new(kasuga, name, n_kasuga)).unwrap();
    store
        .insert(Triple::new(amaterasu, name, n_amaterasu))
        .unwrap();
    store
        .insert(Triple::new(okuninushi, name, n_okuninushi))
        .unwrap();
    store.insert(Triple::new(inari, name, n_inari)).unwrap();
    store
        .insert(Triple::new(takemikazuchi, name, n_takemikazuchi))
        .unwrap();
    store.insert(Triple::new(kojiki, name, n_kojiki)).unwrap();
    store
        .insert(Triple::new(nihon_shoki, name, n_nihon))
        .unwrap();
    store.insert(Triple::new(tanaka, name, n_tanaka)).unwrap();
    store.insert(Triple::new(suzuki, name, n_suzuki)).unwrap();

    // ── Alternate names (only some shrines have them) ──
    store.insert(Triple::new(ise, alt_name, alt_ise)).unwrap();
    store
        .insert(Triple::new(fushimi, alt_name, alt_fushimi))
        .unwrap();

    // ── Founded years (inline integers) ──
    store
        .insert(Triple::new(ise, founded, inline_integer(-4).unwrap()))
        .unwrap(); // 4 BCE
    store
        .insert(Triple::new(izumo, founded, inline_integer(659).unwrap()))
        .unwrap();
    store
        .insert(Triple::new(fushimi, founded, inline_integer(711).unwrap()))
        .unwrap();
    store
        .insert(Triple::new(meiji, founded, inline_integer(1920).unwrap()))
        .unwrap();
    store
        .insert(Triple::new(kasuga, founded, inline_integer(768).unwrap()))
        .unwrap();

    // ── Ranks (1 = highest) ──
    store
        .insert(Triple::new(ise, rank, inline_integer(1).unwrap()))
        .unwrap();
    store
        .insert(Triple::new(izumo, rank, inline_integer(2).unwrap()))
        .unwrap();
    store
        .insert(Triple::new(fushimi, rank, inline_integer(3).unwrap()))
        .unwrap();
    store
        .insert(Triple::new(meiji, rank, inline_integer(4).unwrap()))
        .unwrap();
    store
        .insert(Triple::new(kasuga, rank, inline_integer(5).unwrap()))
        .unwrap();

    // ── Locations ──
    store.insert(Triple::new(ise, located_in, mie)).unwrap();
    store
        .insert(Triple::new(izumo, located_in, shimane))
        .unwrap();
    store
        .insert(Triple::new(fushimi, located_in, kyoto))
        .unwrap();
    store.insert(Triple::new(meiji, located_in, tokyo)).unwrap();
    store.insert(Triple::new(kasuga, located_in, nara)).unwrap();

    // ── Enshrinement relationships ──
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
    // Kasuga also enshrines a second deity for self-join demo
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

    // ── Deities appear in myths ──
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

    // ── People and roles ──
    store
        .insert(Triple::new(tanaka, role, chief_priest))
        .unwrap();
    store.insert(Triple::new(suzuki, role, kannushi)).unwrap();

    // ── RDF-star: annotate enshrinement edges with confidence + source ──
    let qt_ise_ama = quoted_triple_id(ise, enshrines, amaterasu);
    store
        .insert(Triple::new(qt_ise_ama, confidence, conf_99))
        .unwrap();
    store
        .insert(Triple::new(qt_ise_ama, source, src_academic))
        .unwrap();

    let qt_izumo_oku = quoted_triple_id(izumo, enshrines, okuninushi);
    store
        .insert(Triple::new(qt_izumo_oku, confidence, conf_95))
        .unwrap();
    store
        .insert(Triple::new(qt_izumo_oku, source, src_kojiki_text))
        .unwrap();

    let qt_kasuga_ama = quoted_triple_id(kasuga, enshrines, amaterasu);
    store
        .insert(Triple::new(qt_kasuga_ama, confidence, conf_70))
        .unwrap();

    // ── Vector embeddings (4D for simplicity) ──
    // Shrines about sun/nature cluster together, modern shrine is separate
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
        (ise, [0.9, 0.3, 0.1, 0.0]),     // sun-related shrine
        (izumo, [0.7, 0.5, 0.2, 0.1]),   // earth-related shrine
        (fushimi, [0.6, 0.4, 0.8, 0.1]), // harvest shrine
        (meiji, [0.1, 0.1, 0.1, 0.9]),   // modern shrine (very different)
        (kasuga, [0.8, 0.4, 0.2, 0.0]),  // thunder+sun shrine
    ];

    for &(entity, ref vec) in vecs {
        let vec_label = format!(
            "\"vec_{}\"^^<http://loka.dev/f32vec>",
            resolve(&dict, entity)
        );
        let vec_id = dict.intern(&vec_label);
        store
            .insert(Triple::new(entity, has_embedding, vec_id))
            .unwrap();
        vectors.insert(has_embedding, vec.to_vec(), vec_id).unwrap();
    }

    // ── Temporal annotations ──
    // Model: who was the chief priest of Ise at different times?
    //
    // Tanaka was chief priest from year 1900 to 1950
    // Suzuki became chief priest from year 1950 onward (open-ended)
    // Ise enshrining Amaterasu is eternal (no temporal bounds = always visible)
    let serves_at = dict.intern("http://example.org/servesAt");
    store.insert(Triple::new(tanaka, serves_at, ise)).unwrap();
    store.insert(Triple::new(suzuki, serves_at, ise)).unwrap();

    store.insert_temporal(TemporalSignifier::ValidFrom, 1900, tanaka, serves_at, ise);
    store.insert_temporal(TemporalSignifier::ValidTo, 1950, tanaka, serves_at, ise);
    store.insert_temporal(TemporalSignifier::ValidFrom, 1950, suzuki, serves_at, ise);
    // Suzuki has no ValidTo — open-ended (still serving)

    // Fushimi was rebuilt (located in Kyoto from 711, but temporarily in Osaka 1467-1499 during Onin War)
    let osaka = dict.intern("http://example.org/Osaka");
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

    // The main Fushimi-Kyoto location has temporal bounds too
    store.insert_temporal(
        TemporalSignifier::ValidFrom,
        711,
        fushimi,
        located_in,
        kyoto,
    );
    // No ValidTo = still there

    // Meiji Jingu: exists only from 1920 onward
    store.insert_temporal(TemporalSignifier::ValidFrom, 1920, meiji, located_in, tokyo);

    // Pre-intern change type literals for TEMPORAL_DIFF
    dict.intern("\"added\"");
    dict.intern("\"removed\"");
    dict.intern("\"unchanged\"");

    DemoDb {
        store,
        dict,
        vectors,
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                    Loka Feature Demo v0.3.5                      ║");
    println!("║     RDF-star Triplestore · HNSW Vectors · Temporal Queries          ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");

    let db = build_demo_db();
    let s = &db.store;
    let d = &db.dict;
    let v = &db.vectors;

    // ─────────────────────────────────────────────────────────────────────────
    // 1. BASIC TRIPLE PATTERNS
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "1a. All shrines (type lookup via POS index)",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine WHERE {
  ?shrine a ex:Shrine
}"#,
        s,
        d,
        v,
    );

    run_query(
        "1b. Get a specific property (point lookup via SPO index)",
        r#"SELECT ?name WHERE {
  <http://example.org/IseJingu> <http://example.org/name> ?name
}"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 2. JOINS (multi-pattern)
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "2a. Two-hop: shrine -> deity -> domain",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?deity ?domain WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:enshrines ?deity .
  ?deity ex:domain ?domain
}"#,
        s,
        d,
        v,
    );

    run_query(
        "2b. Three-hop: shrine -> deity -> myth (with type constraint)",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?deity ?myth WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:enshrines ?deity .
  ?deity ex:appearsIn ?myth .
  ?myth a ex:Myth
}"#,
        s,
        d,
        v,
    );

    run_query(
        "2c. Self-join: shrines sharing the same deity",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine1 ?shrine2 ?deity WHERE {
  ?shrine1 ex:enshrines ?deity .
  ?shrine2 ex:enshrines ?deity .
  FILTER(?shrine1 != ?shrine2)
}"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 3. FILTER
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "3a. Numeric filter: shrines founded before 800 CE",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name ?year WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:name ?name .
  ?shrine ex:foundedYear ?year .
  FILTER(?year < 800)
}"#,
        s,
        d,
        v,
    );

    run_query(
        "3b. String filter: shrine names containing 'Taisha'",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:name ?name .
  FILTER(CONTAINS(?name, "Taisha"))
}"#,
        s,
        d,
        v,
    );

    run_query(
        "3c. Combined filters: high-rank AND old",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name ?rank ?year WHERE {
  ?shrine ex:name ?name .
  ?shrine ex:rank ?rank .
  ?shrine ex:foundedYear ?year .
  FILTER(?rank <= 2) .
  FILTER(?year < 1000)
}"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 4. OPTIONAL (left join)
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "4a. Shrines with optional alternate names",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name ?altName WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:name ?name .
  OPTIONAL { ?shrine ex:alternateName ?altName }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "4b. Find shrines WITHOUT alternate names",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:name ?name .
  OPTIONAL { ?shrine ex:alternateName ?altName } .
  FILTER(!BOUND(?altName))
}"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 5. UNION
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "5. Union: deities OR people (all named entities)",
        r#"PREFIX ex: <http://example.org/>
SELECT ?entity ?name WHERE {
  { ?entity a ex:Deity . ?entity ex:name ?name }
  UNION
  { ?entity a ex:Person . ?entity ex:name ?name }
}"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 6. AGGREGATES
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "6a. Count shrines per location",
        r#"PREFIX ex: <http://example.org/>
SELECT ?location (COUNT(?shrine) AS ?count) WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:locatedIn ?location
}
GROUP BY ?location"#,
        s,
        d,
        v,
    );

    run_query(
        "6b. Count myths per deity, ordered by most appearances",
        r#"PREFIX ex: <http://example.org/>
SELECT ?deity (COUNT(?myth) AS ?appearances) WHERE {
  ?deity a ex:Deity .
  ?deity ex:appearsIn ?myth
}
GROUP BY ?deity
ORDER BY DESC(?appearances)"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 7. ORDER BY / LIMIT / DISTINCT
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "7a. Top 3 oldest shrines",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name ?year WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:name ?name .
  ?shrine ex:foundedYear ?year
}
ORDER BY ?year
LIMIT 3"#,
        s,
        d,
        v,
    );

    run_query(
        "7b. Distinct locations only",
        r#"PREFIX ex: <http://example.org/>
SELECT DISTINCT ?location WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:locatedIn ?location
}"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 8. RDF-STAR (statements about statements)
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "8a. Enshrinement confidence scores (edge metadata)",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?deity ?conf WHERE {
  ?shrine ex:enshrines ?deity .
  << ?shrine ex:enshrines ?deity >> ex:confidence ?conf
}"#,
        s,
        d,
        v,
    );

    run_query(
        "8b. Provenance: which source backs each enshrinement?",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?deity ?src WHERE {
  ?shrine ex:enshrines ?deity .
  << ?shrine ex:enshrines ?deity >> ex:source ?src
}"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 9. VECTOR SEARCH (HNSW)
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "9a. Find shrines similar to a 'sun/nature' vector",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:name ?name .
  VECTOR_SIMILAR(?shrine ex:hasEmbedding "0.9 0.3 0.1 0.0"^^<http://loka.dev/f32vec>, 0.70)
}
ORDER BY DESC(VECTOR_SCORE(?shrine ex:hasEmbedding "0.9 0.3 0.1 0.0"^^<http://loka.dev/f32vec>))"#,
        s,
        d,
        v,
    );

    run_query(
        "9b. Hybrid: vector search + graph traversal (find similar shrines, then their deities)",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name ?deity WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:name ?name .
  ?shrine ex:enshrines ?deity .
  VECTOR_SIMILAR(?shrine ex:hasEmbedding "0.9 0.3 0.1 0.0"^^<http://loka.dev/f32vec>, 0.70)
}"#,
        s,
        d,
        v,
    );

    run_query(
        "9c. Find the shrine LEAST similar to 'sun' (modern shrine stands out)",
        r#"PREFIX ex: <http://example.org/>
SELECT ?shrine ?name WHERE {
  ?shrine a ex:Shrine .
  ?shrine ex:name ?name .
  VECTOR_SIMILAR(?shrine ex:hasEmbedding "0.9 0.3 0.1 0.0"^^<http://loka.dev/f32vec>, 0.0)
}
ORDER BY VECTOR_SCORE(?shrine ex:hasEmbedding "0.9 0.3 0.1 0.0"^^<http://loka.dev/f32vec>)
LIMIT 1"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // 10. TEMPORAL OPERATORS (world state)
    // ─────────────────────────────────────────────────────────────────────────

    run_query(
        "10a. AT_TIME: Who served at Ise in 1925?",
        r#"SELECT ?person WHERE {
  AT_TIME(1925) {
    ?person <http://example.org/servesAt> <http://example.org/IseJingu> .
  }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "10b. AT_TIME: Who served at Ise in 1975? (after Tanaka's tenure)",
        r#"SELECT ?person WHERE {
  AT_TIME(1975) {
    ?person <http://example.org/servesAt> <http://example.org/IseJingu> .
  }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "10c. DURING: Who served at Ise during 1940-1960? (overlapping tenures)",
        r#"SELECT ?person WHERE {
  DURING(1940, 1960) {
    ?person <http://example.org/servesAt> <http://example.org/IseJingu> .
  }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "10d. AT_TIME: Where was Fushimi Inari located during the Onin War (1470)?",
        r#"SELECT ?location WHERE {
  AT_TIME(1470) {
    <http://example.org/FushimiInari> <http://example.org/locatedIn> ?location .
  }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "10e. AT_TIME: Where is Fushimi Inari now (2024)?",
        r#"SELECT ?location WHERE {
  AT_TIME(2024) {
    <http://example.org/FushimiInari> <http://example.org/locatedIn> ?location .
  }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "10f. WORLD_STATE: All shrine-location facts valid in 1800",
        r#"SELECT ?shrine ?location WHERE {
  WORLD_STATE(1800) {
    ?shrine <http://example.org/locatedIn> ?location .
  }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "10g. WORLD_STATE: Who serves at Ise in 2024? (complete snapshot)",
        r#"SELECT ?person WHERE {
  WORLD_STATE(2024) {
    ?person <http://example.org/servesAt> <http://example.org/IseJingu> .
  }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "10h. TEMPORAL_DIFF: What changed at Ise between 1925 and 1975?",
        r#"SELECT ?person ?change_type WHERE {
  TEMPORAL_DIFF(1925, 1975) {
    ?person <http://example.org/servesAt> <http://example.org/IseJingu> .
  }
}"#,
        s,
        d,
        v,
    );

    run_query(
        "10i. Atemporal facts are always visible (enshrinement has no time bounds)",
        r#"SELECT ?shrine ?deity WHERE {
  AT_TIME(99999) {
    ?shrine <http://example.org/enshrines> ?deity .
  }
}"#,
        s,
        d,
        v,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // Done
    // ─────────────────────────────────────────────────────────────────────────

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!(
        "║  Demo complete. {} triples in graph.                      ║",
        s.len()
    );
    println!("╚══════════════════════════════════════════════════════════════════════╝");
}
