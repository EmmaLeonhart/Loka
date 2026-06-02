package loka

import (
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

func TestNewClient(t *testing.T) {
	client := NewClient("http://localhost:7878")
	if client.Endpoint != "http://localhost:7878" {
		t.Errorf("expected endpoint http://localhost:7878, got %s", client.Endpoint)
	}
	if client.client == nil {
		t.Error("expected http client to be initialized")
	}
}

func TestNewClientStripsTrailingSlash(t *testing.T) {
	client := NewClient("http://localhost:7878/")
	if client.Endpoint != "http://localhost:7878" {
		t.Errorf("expected trailing slash to be stripped, got %s", client.Endpoint)
	}
}

func TestHealth(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" {
			t.Errorf("expected path /health, got %s", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(`{"status":"ok"}`))
	}))
	defer server.Close()

	client := NewClient(server.URL)
	healthy, err := client.Health()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !healthy {
		t.Error("expected health check to return true")
	}
}

func TestHealthUnhealthy(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusServiceUnavailable)
	}))
	defer server.Close()

	client := NewClient(server.URL)
	healthy, err := client.Health()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if healthy {
		t.Error("expected health check to return false for 503")
	}
}

func TestSparql(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/sparql" {
			t.Errorf("expected path /sparql, got %s", r.URL.Path)
		}
		if r.Header.Get("Content-Type") != "application/sparql-query" {
			t.Errorf("expected Content-Type application/sparql-query, got %s", r.Header.Get("Content-Type"))
		}
		w.Header().Set("Content-Type", "application/sparql-results+json")
		w.Write([]byte(`{
			"head": {"vars": ["s", "p", "o"]},
			"results": {"bindings": [
				{
					"s": {"type": "uri", "value": "http://example.org/s1"},
					"p": {"type": "uri", "value": "http://example.org/p1"},
					"o": {"type": "literal", "value": "hello"}
				}
			]}
		}`))
	}))
	defer server.Close()

	client := NewClient(server.URL)
	results, err := client.Sparql("SELECT ?s ?p ?o WHERE { ?s ?p ?o }")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(results.Head.Vars) != 3 {
		t.Errorf("expected 3 vars, got %d", len(results.Head.Vars))
	}
	if len(results.Results.Bindings) != 1 {
		t.Errorf("expected 1 binding row, got %d", len(results.Results.Bindings))
	}
	if results.Results.Bindings[0]["o"].Value != "hello" {
		t.Errorf("expected object value 'hello', got '%s'", results.Results.Bindings[0]["o"].Value)
	}
}

func TestInsertTriples(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/triples" {
			t.Errorf("expected path /triples, got %s", r.URL.Path)
		}
		if r.Header.Get("Content-Type") != "application/n-triples" {
			t.Errorf("expected Content-Type application/n-triples, got %s", r.Header.Get("Content-Type"))
		}
		w.Write([]byte(`{"inserted": 1}`))
	}))
	defer server.Close()

	client := NewClient(server.URL)
	result, err := client.InsertTriples(`<http://example.org/s1> <http://example.org/p1> "hello" .`)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Inserted != 1 {
		t.Errorf("expected 1 inserted, got %d", result.Inserted)
	}
}

func TestServerError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
		w.Write([]byte("bad query"))
	}))
	defer server.Close()

	client := NewClient(server.URL)
	_, err := client.Sparql("INVALID QUERY")
	if err == nil {
		t.Fatal("expected error for bad request")
	}
	lokaErr, ok := err.(*LokaError)
	if !ok {
		t.Fatalf("expected *LokaError, got %T", err)
	}
	if lokaErr.StatusCode != 400 {
		t.Errorf("expected status 400, got %d", lokaErr.StatusCode)
	}
}

func TestRetriesOnServiceUnavailableThenSucceeds(t *testing.T) {
	var calls int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if atomic.AddInt32(&calls, 1) == 1 {
			w.WriteHeader(http.StatusServiceUnavailable)
			w.Write([]byte(`{"error":"unavailable"}`))
			return
		}
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(`{"head":{"vars":[]},"results":{"bindings":[]}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, WithMaxRetries(2), WithRetryBackoff(5*time.Millisecond))
	_, err := client.Sparql("SELECT ?s WHERE { ?s ?p ?o }")
	if err != nil {
		t.Fatalf("expected success after retry, got %v", err)
	}
	if got := atomic.LoadInt32(&calls); got != 2 {
		t.Errorf("expected exactly one retry (2 calls), got %d", got)
	}
}

func TestExhaustsRetriesOnPersistent503(t *testing.T) {
	var calls int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&calls, 1)
		w.WriteHeader(http.StatusServiceUnavailable)
		w.Write([]byte(`{"error":"unavailable"}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, WithMaxRetries(2), WithRetryBackoff(5*time.Millisecond))
	_, err := client.Sparql("SELECT ?s WHERE { ?s ?p ?o }")
	if err == nil {
		t.Fatal("expected error after exhausting retries")
	}
	lokaErr, ok := err.(*LokaError)
	if !ok {
		t.Fatalf("expected *LokaError, got %T", err)
	}
	if lokaErr.StatusCode != 503 {
		t.Errorf("expected status 503, got %d", lokaErr.StatusCode)
	}
	if got := atomic.LoadInt32(&calls); got != 3 {
		t.Errorf("expected initial + 2 retries (3 calls), got %d", got)
	}
}

func TestDoesNotRetryOnClientError(t *testing.T) {
	var calls int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&calls, 1)
		w.WriteHeader(http.StatusBadRequest)
		w.Write([]byte(`{"error":"bad"}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, WithMaxRetries(2), WithRetryBackoff(5*time.Millisecond))
	_, err := client.Sparql("INVALID")
	if err == nil {
		t.Fatal("expected error for 400")
	}
	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Errorf("4xx must not be retried, expected 1 call, got %d", got)
	}
}

func TestRetryDisabledWhenMaxRetriesZero(t *testing.T) {
	var calls int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&calls, 1)
		w.WriteHeader(http.StatusServiceUnavailable)
		w.Write([]byte(`{"error":"unavailable"}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, WithMaxRetries(0))
	if client.MaxRetries() != 0 {
		t.Fatalf("expected MaxRetries 0, got %d", client.MaxRetries())
	}
	_, err := client.Sparql("SELECT ?s WHERE {}")
	if err == nil {
		t.Fatal("expected error")
	}
	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Errorf("maxRetries=0 disables retry, expected 1 call, got %d", got)
	}
}
