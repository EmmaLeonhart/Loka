package loka

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// LokaClient is an HTTP client for communicating with a Loka instance.
type LokaClient struct {
	// Endpoint is the base URL of the Loka instance (no trailing slash).
	Endpoint string

	client       *http.Client
	maxRetries   int
	retryBackoff time.Duration
}

// ClientOption configures a LokaClient at construction time.
type ClientOption func(*LokaClient)

// WithMaxRetries sets how many times a transient failure is retried
// (0 disables retry). Negative values are ignored.
func WithMaxRetries(n int) ClientOption {
	return func(c *LokaClient) {
		if n >= 0 {
			c.maxRetries = n
		}
	}
}

// WithRetryBackoff sets the base delay between retry attempts. The actual
// delay grows linearly: backoff * attemptNumber.
func WithRetryBackoff(d time.Duration) ClientOption {
	return func(c *LokaClient) { c.retryBackoff = d }
}

// WithTimeout sets the per-request HTTP timeout.
func WithTimeout(d time.Duration) ClientOption {
	return func(c *LokaClient) { c.client.Timeout = d }
}

// NewClient creates a new LokaClient pointing at the given endpoint.
//
// The endpoint should be the base URL without a trailing slash,
// e.g. "http://localhost:7878". By default data operations retry transient
// connection failures and HTTP 502/503/504 up to twice with 250ms linear
// backoff; tune via WithMaxRetries / WithRetryBackoff.
func NewClient(endpoint string, opts ...ClientOption) *LokaClient {
	c := &LokaClient{
		Endpoint: strings.TrimRight(endpoint, "/"),
		client: &http.Client{
			Timeout: 30 * time.Second,
		},
		maxRetries:   2,
		retryBackoff: 250 * time.Millisecond,
	}
	for _, opt := range opts {
		opt(c)
	}
	return c
}

// MaxRetries returns the configured number of retries for transient failures.
func (c *LokaClient) MaxRetries() int {
	return c.maxRetries
}

// Health checks whether the Loka instance is reachable and healthy.
// Returns true if the server responds with a 2xx status code.
func (c *LokaClient) Health() (bool, error) {
	resp, err := c.client.Get(c.Endpoint + "/health")
	if err != nil {
		return false, fmt.Errorf("loka: health check failed: %w", err)
	}
	defer resp.Body.Close()
	io.Copy(io.Discard, resp.Body)
	return resp.StatusCode >= 200 && resp.StatusCode < 300, nil
}

// Sparql executes a SPARQL query and returns the parsed JSON result set.
func (c *LokaClient) Sparql(query string) (*SparqlResults, error) {
	resp, err := c.doWithRetry(
		http.MethodPost, "/sparql",
		"application/sparql-query", "application/sparql-results+json",
		[]byte(query),
	)
	if err != nil {
		return nil, fmt.Errorf("loka: SPARQL request failed: %w", err)
	}
	defer resp.Body.Close()

	if err := checkStatus(resp); err != nil {
		return nil, err
	}

	var results SparqlResults
	if err := json.NewDecoder(resp.Body).Decode(&results); err != nil {
		return nil, fmt.Errorf("loka: failed to decode SPARQL results: %w", err)
	}
	return &results, nil
}

// InsertTriples inserts triples in N-Triples format.
func (c *LokaClient) InsertTriples(ntriples string) (*InsertResult, error) {
	resp, err := c.doWithRetry(
		http.MethodPost, "/triples",
		"application/n-triples", "",
		[]byte(ntriples),
	)
	if err != nil {
		return nil, fmt.Errorf("loka: insert request failed: %w", err)
	}
	defer resp.Body.Close()

	if err := checkStatus(resp); err != nil {
		return nil, err
	}

	var result InsertResult
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("loka: failed to decode insert result: %w", err)
	}
	return &result, nil
}

// DeclareVector declares a vector predicate with the given dimensionality.
// Optional HNSW parameters can be provided via VectorOption functions.
func (c *LokaClient) DeclareVector(predicate string, dimensions int, opts ...VectorOption) (*DeclareVectorResult, error) {
	options := &vectorOptions{}
	for _, opt := range opts {
		opt(options)
	}

	body := declareVectorRequest{
		Predicate:          predicate,
		Dimensions:         dimensions,
		HnswM:              options.hnswM,
		HnswEfConstruction: options.hnswEfConstruction,
	}

	var result DeclareVectorResult
	if err := c.postJSON("/vectors/declare", body, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// InsertVector inserts a vector for the given subject under the specified predicate.
// The predicate must have been previously declared with DeclareVector,
// and the vector length must match the declared dimensionality.
func (c *LokaClient) InsertVector(predicate, subject string, vector []float32) (*InsertVectorResult, error) {
	body := insertVectorRequest{
		Predicate: predicate,
		Subject:   subject,
		Vector:    vector,
	}

	var result InsertVectorResult
	if err := c.postJSON("/vectors", body, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// postJSON sends a POST request with a JSON body and decodes the response.
func (c *LokaClient) postJSON(path string, body interface{}, result interface{}) error {
	jsonBody, err := json.Marshal(body)
	if err != nil {
		return fmt.Errorf("loka: failed to marshal request body: %w", err)
	}

	resp, err := c.doWithRetry(http.MethodPost, path, "application/json", "", jsonBody)
	if err != nil {
		return fmt.Errorf("loka: request failed: %w", err)
	}
	defer resp.Body.Close()

	if err := checkStatus(resp); err != nil {
		return err
	}

	if err := json.NewDecoder(resp.Body).Decode(result); err != nil {
		return fmt.Errorf("loka: failed to decode response: %w", err)
	}
	return nil
}

// doWithRetry sends a request, retrying transient failures. A transient
// failure is a connection-level error or an HTTP 502/503/504 response. The
// request is rebuilt from body on every attempt so retrying a POST is safe.
// Backoff grows linearly (retryBackoff * attemptNumber). Non-transient
// responses are returned as-is for the caller to inspect; after the retries
// are exhausted on a connection error, that error is returned.
func (c *LokaClient) doWithRetry(method, path, contentType, accept string, body []byte) (*http.Response, error) {
	var lastErr error
	for attempt := 0; attempt <= c.maxRetries; attempt++ {
		req, err := http.NewRequest(method, c.Endpoint+path, bytes.NewReader(body))
		if err != nil {
			return nil, fmt.Errorf("loka: failed to create request: %w", err)
		}
		if contentType != "" {
			req.Header.Set("Content-Type", contentType)
		}
		if accept != "" {
			req.Header.Set("Accept", accept)
		}

		resp, err := c.client.Do(req)
		if err != nil {
			lastErr = err
			if attempt < c.maxRetries {
				c.backoff(attempt)
				continue
			}
			return nil, err
		}

		if isRetryableStatus(resp.StatusCode) && attempt < c.maxRetries {
			// Drain and close before retrying so the connection can be reused.
			io.Copy(io.Discard, resp.Body)
			resp.Body.Close()
			c.backoff(attempt)
			continue
		}
		return resp, nil
	}
	// Only reached when every attempt failed with a connection error.
	return nil, lastErr
}

// isRetryableStatus reports whether an HTTP status is transient and safe to retry.
func isRetryableStatus(status int) bool {
	return status == http.StatusBadGateway ||
		status == http.StatusServiceUnavailable ||
		status == http.StatusGatewayTimeout
}

// backoff sleeps for retryBackoff * (attempt + 1) before the next attempt.
func (c *LokaClient) backoff(attempt int) {
	d := c.retryBackoff * time.Duration(attempt+1)
	if d > 0 {
		time.Sleep(d)
	}
}

// checkStatus returns a LokaError if the response status is not 2xx.
func checkStatus(resp *http.Response) error {
	if resp.StatusCode >= 200 && resp.StatusCode < 300 {
		return nil
	}
	body, _ := io.ReadAll(resp.Body)
	return &LokaError{
		StatusCode: resp.StatusCode,
		Message:    string(body),
	}
}
