//! # Loka Rust Client
//!
//! A blocking HTTP client for [Loka](https://github.com/EmmaLeonhart/Loka),
//! the RDF-star triplestore with native HNSW vector indexing.
//!
//! ## Quick Start
//!
//! ```no_run
//! use loka::LokaClient;
//!
//! let client = LokaClient::new("http://localhost:7878");
//!
//! // Check health
//! assert!(client.health().unwrap());
//!
//! // Insert triples
//! client.insert_triples(r#"
//!     <http://example.org/paper1> <http://example.org/title> "Graph Databases" .
//! "#).unwrap();
//!
//! // Run a SPARQL query
//! let results = client.sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10").unwrap();
//! for row in &results.results.bindings {
//!     println!("{:?}", row);
//! }
//! ```

pub mod client;
pub mod error;
pub mod types;

pub use client::LokaClient;
pub use error::{Result, LokaError};
