package io.github.emmaleonhart.loka;

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpHandler;
import com.sun.net.httpserver.HttpServer;
import org.json.JSONObject;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Unit tests for {@link LokaClient} using an embedded JDK HTTP server for mocking.
 */
class LokaClientTest {

    private HttpServer server;
    private LokaClient client;

    @BeforeEach
    void setUp() throws IOException {
        server = HttpServer.create(new InetSocketAddress(0), 0);
        server.setExecutor(null);
        server.start();
        int port = server.getAddress().getPort();
        client = new LokaClient("http://localhost:" + port);
    }

    @AfterEach
    void tearDown() {
        server.stop(0);
    }

    // ---- health() tests ----

    @Test
    void healthReturnsTrueOn200() {
        server.createContext("/health", exchange -> {
            respond(exchange, 200, "{\"status\":\"ok\"}");
        });

        assertTrue(client.health());
    }

    @Test
    void healthReturnsFalseOn500() {
        server.createContext("/health", exchange -> {
            respond(exchange, 500, "{\"error\":\"down\"}");
        });

        assertFalse(client.health());
    }

    // ---- sparql() tests ----

    @Test
    void sparqlSendsCorrectContentTypeAndParsesResponse() {
        String sparqlResponse = "{" +
                "\"head\":{\"vars\":[\"s\",\"p\",\"o\"]}," +
                "\"results\":{\"bindings\":[{" +
                "\"s\":{\"type\":\"uri\",\"value\":\"http://example.org/a\"}," +
                "\"p\":{\"type\":\"uri\",\"value\":\"http://example.org/b\"}," +
                "\"o\":{\"type\":\"literal\",\"value\":\"hello\"}" +
                "}]}" +
                "}";

        server.createContext("/sparql", exchange -> {
            // Verify content type
            String contentType = exchange.getRequestHeaders().getFirst("Content-Type");
            assertEquals("application/sparql-query", contentType);

            // Verify request body is the query
            String body = new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8);
            assertEquals("SELECT ?s WHERE { ?s ?p ?o }", body);

            respond(exchange, 200, sparqlResponse);
        });

        SparqlResults results = client.sparql("SELECT ?s WHERE { ?s ?p ?o }");
        assertEquals(3, results.getVariables().size());
        assertEquals(1, results.size());
        assertEquals("http://example.org/a", results.getBindings().get(0).get("s").getValue());
    }

    // ---- insertTriples() tests ----

    @Test
    void insertTriplesSendsNTriplesContentType() {
        String ntriples = "<http://ex.org/s> <http://ex.org/p> \"value\" .";

        server.createContext("/triples", exchange -> {
            String contentType = exchange.getRequestHeaders().getFirst("Content-Type");
            assertEquals("application/n-triples", contentType);

            String body = new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8);
            assertEquals(ntriples, body);

            respond(exchange, 200, "{\"inserted\":1}");
        });

        JSONObject result = client.insertTriples(ntriples);
        assertEquals(1, result.getInt("inserted"));
    }

    // ---- declareVector() tests ----

    @Test
    void declareVectorSendsCorrectJson() {
        server.createContext("/vectors/declare", exchange -> {
            String contentType = exchange.getRequestHeaders().getFirst("Content-Type");
            assertEquals("application/json", contentType);

            String body = new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8);
            JSONObject json = new JSONObject(body);
            assertEquals("http://ex.org/hasEmbed", json.getString("predicate"));
            assertEquals(768, json.getInt("dimensions"));
            assertEquals(16, json.getInt("hnswM"));
            assertEquals(200, json.getInt("hnswEfConstruction"));

            respond(exchange, 200, "{\"ok\":true}");
        });

        JSONObject result = client.declareVector("http://ex.org/hasEmbed", 768);
        assertTrue(result.getBoolean("ok"));
    }

    @Test
    void declareVectorWithCustomHnswParams() {
        server.createContext("/vectors/declare", exchange -> {
            String body = new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8);
            JSONObject json = new JSONObject(body);
            assertEquals(32, json.getInt("hnswM"));
            assertEquals(400, json.getInt("hnswEfConstruction"));

            respond(exchange, 200, "{\"ok\":true}");
        });

        client.declareVector("http://ex.org/hasEmbed", 768, 32, 400);
    }

    // ---- insertVector() tests ----

    @Test
    void insertVectorSendsCorrectJsonWithVectorArray() {
        server.createContext("/vectors", exchange -> {
            String contentType = exchange.getRequestHeaders().getFirst("Content-Type");
            assertEquals("application/json", contentType);

            String body = new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8);
            JSONObject json = new JSONObject(body);
            assertEquals("http://ex.org/pred", json.getString("predicate"));
            assertEquals("http://ex.org/subj", json.getString("subject"));
            assertEquals(3, json.getJSONArray("vector").length());

            respond(exchange, 200, "{\"ok\":true}");
        });

        double[] vec = {0.1, 0.2, 0.3};
        JSONObject result = client.insertVector("http://ex.org/pred", "http://ex.org/subj", vec);
        assertTrue(result.getBoolean("ok"));
    }

    // ---- rebuildHnsw() tests ----

    @Test
    void rebuildHnswCallsPostVectorsRebuild() {
        server.createContext("/vectors/rebuild", exchange -> {
            assertEquals("POST", exchange.getRequestMethod());
            respond(exchange, 200, "{\"rebuilt\":true}");
        });

        JSONObject result = client.rebuildHnsw();
        assertTrue(result.getBoolean("rebuilt"));
    }

    // ---- healthReport() tests ----

    @Test
    void healthReportCombinesHealthAndVectorHealth() {
        server.createContext("/health", exchange -> {
            respond(exchange, 200, "{\"status\":\"ok\"}");
        });
        server.createContext("/vectors/health", exchange -> {
            respond(exchange, 200, "{\"indexes\":2,\"totalVectors\":1000}");
        });

        JSONObject report = client.healthReport();
        assertTrue(report.getBoolean("healthy"));
        assertEquals(2, report.getJSONObject("vectors").getInt("indexes"));
        assertEquals(1000, report.getJSONObject("vectors").getInt("totalVectors"));
    }

    // ---- error handling tests ----

    @Test
    void sparqlThrowsLokaErrorOn400() {
        server.createContext("/sparql", exchange -> {
            respond(exchange, 400, "{\"error\":\"Bad query\"}");
        });

        LokaError error = assertThrows(LokaError.class, () ->
                client.sparql("INVALID QUERY"));
        assertEquals(400, error.getStatusCode());
        assertTrue(error.getMessage().contains("400"));
    }

    @Test
    void insertTriplesThrowsLokaErrorOn500() {
        server.createContext("/triples", exchange -> {
            respond(exchange, 500, "{\"error\":\"Internal error\"}");
        });

        LokaError error = assertThrows(LokaError.class, () ->
                client.insertTriples("<s> <p> <o> ."));
        assertEquals(500, error.getStatusCode());
    }

    @Test
    void connectionRefusedThrowsLokaError() {
        // Use a client pointing at a port that is not listening
        LokaClient badClient = new LokaClient("http://localhost:1");
        assertThrows(LokaError.class, badClient::health);
    }

    @Test
    void getEndpointReturnsConfiguredUrl() {
        assertEquals("http://localhost:" + server.getAddress().getPort(), client.getEndpoint());
    }

    @Test
    void endpointTrailingSlashIsStripped() {
        LokaClient c = new LokaClient("http://localhost:9999/");
        assertEquals("http://localhost:9999", c.getEndpoint());
    }

    // ---- retry logic tests ----

    /** A client pointing at the test server with fast (5ms) backoff for retry tests. */
    private LokaClient retryClient(int maxRetries) {
        int port = server.getAddress().getPort();
        return new LokaClient("http://localhost:" + port,
                Duration.ofSeconds(5), maxRetries, Duration.ofMillis(5));
    }

    @Test
    void retriesOnServiceUnavailableThenSucceeds() {
        AtomicInteger calls = new AtomicInteger(0);
        server.createContext("/sparql", exchange -> {
            int n = calls.incrementAndGet();
            if (n == 1) {
                respond(exchange, 503, "{\"error\":\"unavailable\"}");
            } else {
                respond(exchange, 200,
                        "{\"head\":{\"vars\":[]},\"results\":{\"bindings\":[]}}");
            }
        });

        SparqlResults results = retryClient(2).sparql("SELECT ?s WHERE { ?s ?p ?o }");
        assertEquals(0, results.size());
        assertEquals(2, calls.get(), "should have retried exactly once after the 503");
    }

    @Test
    void exhaustsRetriesOnPersistent503AndThrows() {
        AtomicInteger calls = new AtomicInteger(0);
        server.createContext("/sparql", exchange -> {
            calls.incrementAndGet();
            respond(exchange, 503, "{\"error\":\"unavailable\"}");
        });

        LokaError error = assertThrows(LokaError.class, () ->
                retryClient(2).sparql("SELECT ?s WHERE { ?s ?p ?o }"));
        assertEquals(503, error.getStatusCode());
        assertEquals(3, calls.get(), "initial attempt + 2 retries");
    }

    @Test
    void doesNotRetryOnClientError() {
        AtomicInteger calls = new AtomicInteger(0);
        server.createContext("/sparql", exchange -> {
            calls.incrementAndGet();
            respond(exchange, 400, "{\"error\":\"bad query\"}");
        });

        LokaError error = assertThrows(LokaError.class, () ->
                retryClient(2).sparql("INVALID"));
        assertEquals(400, error.getStatusCode());
        assertEquals(1, calls.get(), "4xx is not transient and must not be retried");
    }

    @Test
    void doesNotRetryOnInternalServerError() {
        AtomicInteger calls = new AtomicInteger(0);
        server.createContext("/triples", exchange -> {
            calls.incrementAndGet();
            respond(exchange, 500, "{\"error\":\"internal\"}");
        });

        assertThrows(LokaError.class, () -> retryClient(2).insertTriples("<s> <p> <o> ."));
        assertEquals(1, calls.get(), "500 is not treated as transient");
    }

    @Test
    void retryDisabledWhenMaxRetriesZero() {
        AtomicInteger calls = new AtomicInteger(0);
        server.createContext("/sparql", exchange -> {
            calls.incrementAndGet();
            respond(exchange, 503, "{\"error\":\"unavailable\"}");
        });

        assertThrows(LokaError.class, () -> retryClient(0).sparql("SELECT ?s WHERE {}"));
        assertEquals(1, calls.get(), "maxRetries=0 disables retry");
    }

    @Test
    void getMaxRetriesReturnsConfiguredValue() {
        LokaClient c = new LokaClient("http://localhost:9999", Duration.ofSeconds(5), 4);
        assertEquals(4, c.getMaxRetries());
        assertEquals(LokaClient.DEFAULT_MAX_RETRIES, new LokaClient("http://localhost:9999").getMaxRetries());
    }

    // ---- OWL validation wiring tests ----

    /**
     * Register a /sparql context that answers the OWLValidator's load queries:
     * one rdfs:domain axiom and one rdf:type fact; everything else empty.
     */
    private void installOwlSparql(String prop, String domainClass, String entity, String entityType) {
        server.createContext("/sparql", exchange -> {
            String body = new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8);
            if (body.contains(OWLValidator.OWL_FUNCTIONAL)) {
                respond(exchange, 200, owlRows("[\"p\"]", ""));
            } else if (body.contains(OWLValidator.RDFS_DOMAIN)) {
                respond(exchange, 200, owlRows("[\"p\",\"d\"]",
                        uriBind("p", prop) + "," + uriBind("d", domainClass)));
            } else if (body.contains(OWLValidator.RDF_TYPE)) {
                respond(exchange, 200, owlRows("[\"e\",\"t\"]",
                        uriBind("e", entity) + "," + uriBind("t", entityType)));
            } else {
                respond(exchange, 200, owlRows("[]", ""));
            }
        });
    }

    @Test
    void insertTriplesRaisesOnOwlViolation() {
        installOwlSparql("http://ex.org/worksAt", "http://ex.org/Person",
                "http://ex.org/car1", "http://ex.org/Car");
        // /triples should not be reached, but register it so a miss would be obvious.
        server.createContext("/triples", exchange -> respond(exchange, 200, "{\"inserted\":1}"));

        OWLViolation violation = assertThrows(OWLViolation.class, () -> client.insertTriples(
                "<http://ex.org/car1> <http://ex.org/worksAt> <http://ex.org/company1> ."));
        assertEquals("domain", violation.getConstraintType());
    }

    @Test
    void insertTriplesSkipsValidationWhenDisabled() {
        installOwlSparql("http://ex.org/worksAt", "http://ex.org/Person",
                "http://ex.org/car1", "http://ex.org/Car");
        server.createContext("/triples", exchange -> respond(exchange, 200, "{\"inserted\":1}"));

        client.setOwlValidation(false);
        // The same triple that violates above must now sail through.
        JSONObject result = client.insertTriples(
                "<http://ex.org/car1> <http://ex.org/worksAt> <http://ex.org/company1> .");
        assertEquals(1, result.getInt("inserted"));
    }

    @Test
    void insertTriplesProceedsWhenNoConstraints() {
        // /sparql answers every load query with empty bindings → no constraints.
        server.createContext("/sparql", exchange -> {
            // drain the request body so the exchange completes cleanly
            exchange.getRequestBody().readAllBytes();
            respond(exchange, 200, owlRows("[]", ""));
        });
        server.createContext("/triples", exchange -> respond(exchange, 200, "{\"inserted\":1}"));

        JSONObject result = client.insertTriples(
                "<http://ex.org/a> <http://ex.org/b> <http://ex.org/c> .");
        assertEquals(1, result.getInt("inserted"));
    }

    @Test
    void owlValidationDefaultsOnAndCanBeDisabled() {
        assertTrue(new LokaClient("http://localhost:9999").isOwlValidation());
        LokaClient c = new LokaClient("http://localhost:9999");
        c.setOwlValidation(false);
        assertFalse(c.isOwlValidation());
    }

    private static String uriBind(String var, String value) {
        return "\"" + var + "\":{\"type\":\"uri\",\"value\":\"" + value + "\"}";
    }

    private static String owlRows(String varsJson, String rowInner) {
        String rows = rowInner.isEmpty() ? "" : "{" + rowInner + "}";
        return "{\"head\":{\"vars\":" + varsJson + "},\"results\":{\"bindings\":[" + rows + "]}}";
    }

    // ---- helper ----

    private static void respond(HttpExchange exchange, int status, String body) throws IOException {
        byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        exchange.getResponseHeaders().set("Content-Type", "application/json");
        exchange.sendResponseHeaders(status, bytes.length);
        try (OutputStream os = exchange.getResponseBody()) {
            os.write(bytes);
        }
    }
}
