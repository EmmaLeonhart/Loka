# loka

TypeScript/JavaScript client for [Loka](https://github.com/EmmaLeonhart/Loka) — an RDF-star triplestore with native HNSW vector indexing.

## Installation

```bash
npm install loka
```

## Quick Start

```typescript
import { LokaClient } from "loka";

const client = new LokaClient("http://localhost:3030");

// Check server health
if (await client.health()) {
  console.log("Loka is running");
}

// Run a SPARQL query
const results = await client.sparql(`
  SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10
`);
for (const binding of results.results.bindings) {
  console.log(binding.s.value, binding.p.value, binding.o.value);
}

// Insert triples
await client.insertTriples(`
  <http://example.org/paper/1> <http://example.org/title> "Attention Is All You Need" .
  <http://example.org/paper/1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Paper> .
`);

// Declare a vector predicate
await client.declareVector("http://example.org/hasEmbedding", 1536);

// Insert a vector
await client.insertVector(
  "http://example.org/hasEmbedding",
  "http://example.org/paper/1",
  [0.23, -0.11, 0.87 /* ... 1536 dimensions */]
);
```

## License

Apache-2.0
