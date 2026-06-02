package io.github.emmaleonhart.loka;

import org.json.JSONObject;

import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.StringJoiner;

/**
 * Synchronous client for communicating with a Loka instance.
 *
 * <p>Uses the built-in {@link java.net.http.HttpClient} (Java 11+)
 * and {@link org.json.JSONObject} for JSON handling.</p>
 *
 * <h3>Example</h3>
 * <pre>{@code
 * LokaClient client = new LokaClient("http://localhost:7878");
 * boolean healthy = client.health();
 * SparqlResults results = client.sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 5");
 * }</pre>
 */
public class LokaClient {

    /** Default connection timeout when not specified. */
    public static final Duration DEFAULT_CONNECT_TIMEOUT = Duration.ofSeconds(10);
    /** Default number of retries on transient failures when not specified. */
    public static final int DEFAULT_MAX_RETRIES = 2;
    /** Default base backoff between retry attempts when not specified. */
    public static final Duration DEFAULT_RETRY_BACKOFF = Duration.ofMillis(250);

    private final String endpoint;
    private final HttpClient httpClient;
    private final int maxRetries;
    private final Duration retryBackoff;

    // Client-side OWL validation (enabled by default, like the Python SDK).
    // The ontology is loaded lazily from the database on first insert.
    private boolean owlValidation = true;
    private OWLValidator owlValidator;

    /**
     * Create a new client pointing at the given Loka endpoint, using the
     * default connect timeout (10s), retry count (2), and backoff (250ms).
     *
     * @param endpoint base URL without trailing slash, e.g. {@code "http://localhost:7878"}
     */
    public LokaClient(String endpoint) {
        this(endpoint, DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_RETRIES, DEFAULT_RETRY_BACKOFF);
    }

    /**
     * Create a client with a custom connect timeout and retry count, using the
     * default retry backoff (250ms).
     *
     * @param endpoint       base URL without trailing slash
     * @param connectTimeout how long to wait when establishing a connection
     * @param maxRetries     how many times to retry a transient failure (0 disables retry)
     */
    public LokaClient(String endpoint, Duration connectTimeout, int maxRetries) {
        this(endpoint, connectTimeout, maxRetries, DEFAULT_RETRY_BACKOFF);
    }

    /**
     * Create a fully-configured client.
     *
     * <p>Retries apply to transient connection failures ({@link IOException})
     * and to transient HTTP statuses (502, 503, 504). They do <em>not</em>
     * apply to 4xx or 500 responses, which are not considered transient.
     * Backoff grows linearly: {@code retryBackoff * attemptNumber}.</p>
     *
     * @param endpoint       base URL without trailing slash
     * @param connectTimeout how long to wait when establishing a connection
     * @param maxRetries     how many times to retry a transient failure (0 disables retry)
     * @param retryBackoff   base delay between attempts (grows linearly per attempt)
     */
    public LokaClient(String endpoint, Duration connectTimeout, int maxRetries, Duration retryBackoff) {
        this.endpoint = endpoint.replaceAll("/+$", "");
        this.maxRetries = Math.max(0, maxRetries);
        this.retryBackoff = retryBackoff;
        this.httpClient = HttpClient.newBuilder()
                .connectTimeout(connectTimeout)
                .build();
    }

    // ---- OWL validation ----

    /**
     * Enable or disable client-side OWL validation (enabled by default).
     *
     * <p>When enabled, {@link #insertTriples(String)} loads the OWL ontology
     * from the database on first use and rejects triples that violate
     * {@code rdfs:domain}/{@code rdfs:range}/{@code owl:disjointWith}
     * constraints before sending them.</p>
     *
     * @param enabled whether to validate
     */
    public void setOwlValidation(boolean enabled) {
        this.owlValidation = enabled;
    }

    /** Whether client-side OWL validation is enabled. */
    public boolean isOwlValidation() {
        return owlValidation;
    }

    /** Force a reload of the OWL ontology from the database on next validation. */
    public void reloadOwl() {
        this.owlValidator = null;
        ensureOwlLoaded();
    }

    /**
     * Lazily load the OWL ontology from the database. If it cannot be loaded
     * (e.g. the endpoint is unreachable or has no SPARQL support), validation
     * is skipped silently — matching the Python SDK.
     */
    private void ensureOwlLoaded() {
        if (owlValidator != null) {
            return;
        }
        try {
            OWLValidator validator = new OWLValidator();
            validator.loadFromClient(this);
            owlValidator = validator;
        } catch (RuntimeException e) {
            owlValidator = null;
        }
    }

    /**
     * Check whether the Loka instance is reachable and healthy.
     *
     * @return true if the server returns a 2xx status
     * @throws LokaError if the request fails
     */
    public boolean health() {
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(endpoint + "/health"))
                .GET()
                .timeout(Duration.ofSeconds(5))
                .build();

        HttpResponse<String> response = send(request);
        return response.statusCode() >= 200 && response.statusCode() < 300;
    }

    /**
     * Execute a SPARQL query and return parsed results.
     *
     * @param query a SPARQL query string
     * @return parsed SPARQL results
     * @throws LokaError if the query fails or the server returns an error
     */
    public SparqlResults sparql(String query) {
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(endpoint + "/sparql"))
                .header("Content-Type", "application/sparql-query")
                .header("Accept", "application/sparql-results+json")
                .POST(HttpRequest.BodyPublishers.ofString(query))
                .timeout(Duration.ofSeconds(30))
                .build();

        HttpResponse<String> response = send(request);
        requireSuccess(response);
        return new SparqlResults(new JSONObject(response.body()));
    }

    /**
     * Insert triples in N-Triples format.
     *
     * @param ntriples valid N-Triples data
     * @return the server response as a JSONObject
     * @throws LokaError if the insertion fails
     */
    public JSONObject insertTriples(String ntriples) {
        // Client-side OWL validation before sending (lean store, smart client).
        if (owlValidation) {
            ensureOwlLoaded();
            if (owlValidator != null && owlValidator.hasConstraints()) {
                java.util.List<OWLViolation> violations = owlValidator.validateNtriples(ntriples);
                if (!violations.isEmpty()) {
                    throw violations.get(0); // raise the first violation
                }
            }
        }

        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(endpoint + "/triples"))
                .header("Content-Type", "application/n-triples")
                .POST(HttpRequest.BodyPublishers.ofString(ntriples))
                .timeout(Duration.ofSeconds(30))
                .build();

        HttpResponse<String> response = send(request);
        requireSuccess(response);
        return new JSONObject(response.body());
    }

    /**
     * Declare a vector predicate with the given dimensionality.
     *
     * @param predicate  the predicate IRI
     * @param dimensions the vector dimensionality
     * @return the server response as a JSONObject
     * @throws LokaError if the declaration fails
     */
    public JSONObject declareVector(String predicate, int dimensions) {
        return declareVector(predicate, dimensions, 16, 200);
    }

    /**
     * Declare a vector predicate with full HNSW parameters.
     *
     * @param predicate        the predicate IRI
     * @param dimensions       the vector dimensionality
     * @param hnswM            max connections per node per layer
     * @param hnswEfConstruction beam width during index construction
     * @return the server response as a JSONObject
     * @throws LokaError if the declaration fails
     */
    public JSONObject declareVector(String predicate, int dimensions, int hnswM, int hnswEfConstruction) {
        JSONObject body = new JSONObject();
        body.put("predicate", predicate);
        body.put("dimensions", dimensions);
        body.put("hnswM", hnswM);
        body.put("hnswEfConstruction", hnswEfConstruction);

        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(endpoint + "/vectors/declare"))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(body.toString()))
                .timeout(Duration.ofSeconds(10))
                .build();

        HttpResponse<String> response = send(request);
        requireSuccess(response);
        return new JSONObject(response.body());
    }

    /**
     * Insert a vector for the given subject under the specified predicate.
     *
     * @param predicate the predicate IRI (must be previously declared)
     * @param subject   the subject IRI
     * @param vector    the embedding vector
     * @return the server response as a JSONObject
     * @throws LokaError if the insertion fails
     */
    public JSONObject insertVector(String predicate, String subject, double[] vector) {
        JSONObject body = new JSONObject();
        body.put("predicate", predicate);
        body.put("subject", subject);

        // Build the vector array manually to avoid JSONArray boxing issues
        StringJoiner sj = new StringJoiner(",", "[", "]");
        for (double v : vector) {
            sj.add(String.valueOf(v));
        }

        // Construct full JSON with raw array to preserve numeric precision
        String json = String.format(
                "{\"predicate\":%s,\"subject\":%s,\"vector\":%s}",
                JSONObject.quote(predicate),
                JSONObject.quote(subject),
                sj.toString()
        );

        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(endpoint + "/vectors"))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(json))
                .timeout(Duration.ofSeconds(10))
                .build();

        HttpResponse<String> response = send(request);
        requireSuccess(response);
        return new JSONObject(response.body());
    }

    /**
     * Compact and rebuild all HNSW indexes on the server.
     *
     * <p>This operation may take a long time depending on the number
     * of indexed vectors. A 60-second timeout is used.</p>
     *
     * @return the server response as a JSONObject
     * @throws LokaError if the rebuild fails
     */
    public JSONObject rebuildHnsw() {
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(endpoint + "/vectors/rebuild"))
                .POST(HttpRequest.BodyPublishers.noBody())
                .timeout(Duration.ofSeconds(60))
                .build();

        HttpResponse<String> response = send(request);
        requireSuccess(response);
        return new JSONObject(response.body());
    }

    /**
     * Get a combined health report including general health and vector index status.
     *
     * <p>Calls both {@code GET /health} and {@code GET /vectors/health},
     * returning a single JSON object with keys {@code "healthy"} (boolean)
     * and {@code "vectors"} (vector index details).</p>
     *
     * @return a combined health report as a JSONObject
     * @throws LokaError if either health endpoint fails
     */
    public JSONObject healthReport() {
        // Check general health
        HttpRequest healthReq = HttpRequest.newBuilder()
                .uri(URI.create(endpoint + "/health"))
                .GET()
                .timeout(Duration.ofSeconds(5))
                .build();

        HttpResponse<String> healthResp = send(healthReq);
        boolean healthy = healthResp.statusCode() >= 200 && healthResp.statusCode() < 300;

        // Get vector health details
        HttpRequest vectorReq = HttpRequest.newBuilder()
                .uri(URI.create(endpoint + "/vectors/health"))
                .GET()
                .header("Accept", "application/json")
                .timeout(Duration.ofSeconds(10))
                .build();

        HttpResponse<String> vectorResp = send(vectorReq);
        requireSuccess(vectorResp);

        JSONObject report = new JSONObject();
        report.put("healthy", healthy);
        report.put("vectors", new JSONObject(vectorResp.body()));
        return report;
    }

    /**
     * Return the base endpoint URL this client is configured with.
     *
     * @return the endpoint URL
     */
    public String getEndpoint() {
        return endpoint;
    }

    /**
     * Return the configured maximum number of retries for transient failures.
     *
     * @return the max retry count
     */
    public int getMaxRetries() {
        return maxRetries;
    }

    // ---- internal helpers ----

    /**
     * Send a request, retrying on transient failures.
     *
     * <p>A transient failure is a connection-level {@link IOException} or an
     * HTTP 502/503/504 response. Up to {@code maxRetries} additional attempts
     * are made with linearly-growing backoff. Non-transient responses (2xx,
     * 4xx, 500) are returned as-is on the first attempt. After the retries are
     * exhausted, the last response is returned (so {@code requireSuccess} can
     * surface the status) or, for a persistent IOException, a {@link LokaError}
     * is thrown.</p>
     */
    private HttpResponse<String> send(HttpRequest request) {
        LokaError lastIoError = null;
        for (int attempt = 0; attempt <= maxRetries; attempt++) {
            try {
                HttpResponse<String> response =
                        httpClient.send(request, HttpResponse.BodyHandlers.ofString());
                if (isRetryableStatus(response.statusCode()) && attempt < maxRetries) {
                    backoff(attempt);
                    continue;
                }
                return response;
            } catch (IOException e) {
                lastIoError = new LokaError("HTTP request failed: " + e.getMessage(), e);
                if (attempt < maxRetries) {
                    backoff(attempt);
                    continue;
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new LokaError("HTTP request interrupted", e);
            }
        }
        // Only reached when every attempt threw an IOException.
        throw lastIoError != null
                ? lastIoError
                : new LokaError("HTTP request failed after " + (maxRetries + 1) + " attempts", (Throwable) null);
    }

    /** 502/503/504 are treated as transient and safe to retry. */
    private static boolean isRetryableStatus(int status) {
        return status == 502 || status == 503 || status == 504;
    }

    /** Sleep for {@code retryBackoff * (attempt + 1)} before the next attempt. */
    private void backoff(int attempt) {
        long millis = retryBackoff.toMillis() * (attempt + 1L);
        if (millis <= 0) {
            return;
        }
        try {
            Thread.sleep(millis);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new LokaError("Retry backoff interrupted", e);
        }
    }

    private void requireSuccess(HttpResponse<String> response) {
        int status = response.statusCode();
        if (status < 200 || status >= 300) {
            throw new LokaError(
                    "Loka returned HTTP " + status + ": " + response.body(),
                    status
            );
        }
    }
}
