# loka

Rust client for [Loka](https://github.com/EmmaLeonhart/Loka) — an RDF-star triplestore with native HNSW vector indexing.

## Installation

```sh
cargo add loka
```

## Usage

```rust
use loka::LokaClient;

fn main() -> loka::Result<()> {
    let client = LokaClient::new("http://localhost:7878");

    // Health check
    assert!(client.health()?);

    // Insert triples (N-Triples format)
    client.insert_triples(
        r#"<http://example.org/paper1> <http://example.org/title> "Graph Databases" ."#,
    )?;

    // SPARQL query
    let results = client.sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10")?;
    for row in &results.results.bindings {
        for (var, val) in row {
            println!("  {} = {}", var, val.value);
        }
    }

    // Declare a vector predicate
    client.declare_vector("http://example.org/hasEmbedding", 1536, None, None)?;

    // Insert a vector
    let embedding: Vec<f32> = vec![0.0; 1536];
    client.insert_vector(
        "http://example.org/hasEmbedding",
        "http://example.org/paper1",
        &embedding,
    )?;

    Ok(())
}
```

## License

Apache-2.0
