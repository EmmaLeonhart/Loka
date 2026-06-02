using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Loka.Client;

/// <summary>
/// Async client for communicating with a Loka instance.
/// </summary>
/// <example>
/// <code>
/// var client = new LokaClient("http://localhost:7878");
/// var healthy = await client.HealthAsync();
/// var results = await client.SparqlAsync("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10");
/// </code>
/// </example>
public class LokaClient : IDisposable
{
    private readonly HttpClient _http;
    private readonly string _endpoint;
    private readonly JsonSerializerOptions _jsonOptions;
    private readonly int _maxRetries;
    private readonly TimeSpan _retryBackoff;
    private bool _disposed;

    /// <summary>
    /// Create a new client pointing at the given Loka endpoint.
    /// </summary>
    /// <param name="endpoint">Base URL without trailing slash, e.g. "http://localhost:7878"</param>
    /// <param name="httpClient">Optional pre-configured HttpClient. If null, a new one is created.</param>
    /// <param name="maxRetries">
    /// How many times a transient failure is retried on data operations
    /// (0 disables retry). Default 2. Negative values are clamped to 0.
    /// </param>
    /// <param name="retryBackoff">
    /// Base delay between retry attempts (grows linearly: backoff * attemptNumber).
    /// Default 250ms.
    /// </param>
    public LokaClient(
        string endpoint,
        HttpClient? httpClient = null,
        int maxRetries = 2,
        TimeSpan? retryBackoff = null)
    {
        _endpoint = endpoint.TrimEnd('/');
        _http = httpClient ?? new HttpClient();
        _http.DefaultRequestHeaders.UserAgent.ParseAdd("loka-dotnet-sdk/0.1.0");
        _maxRetries = Math.Max(0, maxRetries);
        _retryBackoff = retryBackoff ?? TimeSpan.FromMilliseconds(250);
        _jsonOptions = new JsonSerializerOptions
        {
            PropertyNameCaseInsensitive = true,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        };
    }

    /// <summary>
    /// The base endpoint URL this client is configured with.
    /// </summary>
    public string Endpoint => _endpoint;

    /// <summary>
    /// The configured number of retries for transient failures on data operations.
    /// </summary>
    public int MaxRetries => _maxRetries;

    /// <summary>
    /// Check whether the Loka instance is reachable and healthy.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>True if the server responds with a 2xx status.</returns>
    public async Task<bool> HealthAsync(CancellationToken cancellationToken = default)
    {
        try
        {
            var response = await _http.GetAsync($"{_endpoint}/health", cancellationToken);
            return response.IsSuccessStatusCode;
        }
        catch (HttpRequestException)
        {
            return false;
        }
    }

    /// <summary>
    /// Execute a SPARQL query and return parsed results.
    /// </summary>
    /// <param name="query">A SPARQL query string.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Parsed SPARQL results.</returns>
    /// <exception cref="LokaException">If the query fails.</exception>
    public async Task<SparqlResults> SparqlAsync(string query, CancellationToken cancellationToken = default)
    {
        var response = await SendWithRetryAsync(() =>
        {
            var request = new HttpRequestMessage(HttpMethod.Post, $"{_endpoint}/sparql")
            {
                Content = new StringContent(query, Encoding.UTF8, "application/sparql-query"),
            };
            request.Headers.Accept.ParseAdd("application/sparql-results+json");
            return request;
        }, cancellationToken);
        await EnsureSuccessAsync(response, cancellationToken);

        var results = await response.Content.ReadFromJsonAsync<SparqlResults>(_jsonOptions, cancellationToken);
        return results ?? throw new LokaException("Empty response from server");
    }

    /// <summary>
    /// Insert triples in N-Triples format.
    /// </summary>
    /// <param name="ntriples">Valid N-Triples data.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Insert result with count of inserted triples.</returns>
    /// <exception cref="LokaException">If the insertion fails.</exception>
    public async Task<InsertResult> InsertTriplesAsync(string ntriples, CancellationToken cancellationToken = default)
    {
        var response = await SendWithRetryAsync(() => new HttpRequestMessage(HttpMethod.Post, $"{_endpoint}/triples")
        {
            Content = new StringContent(ntriples, Encoding.UTF8, "application/n-triples"),
        }, cancellationToken);
        await EnsureSuccessAsync(response, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<InsertResult>(_jsonOptions, cancellationToken);
        return result ?? throw new LokaException("Empty response from server");
    }

    /// <summary>
    /// Declare a vector predicate with the given dimensionality.
    /// </summary>
    /// <param name="predicate">The predicate IRI.</param>
    /// <param name="dimensions">Vector dimensionality.</param>
    /// <param name="hnswM">Max connections per node per layer (default: 16).</param>
    /// <param name="hnswEfConstruction">Beam width during index construction (default: 200).</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Declaration result.</returns>
    /// <exception cref="LokaException">If the declaration fails.</exception>
    public async Task<DeclareVectorResult> DeclareVectorAsync(
        string predicate,
        int dimensions,
        int? hnswM = null,
        int? hnswEfConstruction = null,
        CancellationToken cancellationToken = default)
    {
        var body = new DeclareVectorRequest(predicate, dimensions, hnswM, hnswEfConstruction);
        var json = JsonSerializer.Serialize(body, _jsonOptions);
        var response = await SendWithRetryAsync(() => new HttpRequestMessage(HttpMethod.Post, $"{_endpoint}/vectors/declare")
        {
            Content = new StringContent(json, Encoding.UTF8, "application/json"),
        }, cancellationToken);
        await EnsureSuccessAsync(response, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<DeclareVectorResult>(_jsonOptions, cancellationToken);
        return result ?? throw new LokaException("Empty response from server");
    }

    /// <summary>
    /// Insert a vector for the given subject under the specified predicate.
    /// </summary>
    /// <param name="predicate">The predicate IRI (must be previously declared).</param>
    /// <param name="subject">The subject IRI.</param>
    /// <param name="vector">The embedding vector.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Insertion result.</returns>
    /// <exception cref="LokaException">If the insertion fails.</exception>
    public async Task<InsertVectorResult> InsertVectorAsync(
        string predicate,
        string subject,
        float[] vector,
        CancellationToken cancellationToken = default)
    {
        var body = new InsertVectorRequest(predicate, subject, vector);
        var json = JsonSerializer.Serialize(body, _jsonOptions);
        var response = await SendWithRetryAsync(() => new HttpRequestMessage(HttpMethod.Post, $"{_endpoint}/vectors")
        {
            Content = new StringContent(json, Encoding.UTF8, "application/json"),
        }, cancellationToken);
        await EnsureSuccessAsync(response, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<InsertVectorResult>(_jsonOptions, cancellationToken);
        return result ?? throw new LokaException("Empty response from server");
    }

    /// <summary>
    /// Dispose the underlying HttpClient if it was created by this instance.
    /// </summary>
    public void Dispose()
    {
        if (!_disposed)
        {
            _http.Dispose();
            _disposed = true;
        }
        GC.SuppressFinalize(this);
    }

    /// <summary>
    /// Send a request built fresh on each attempt, retrying transient failures.
    /// A transient failure is an <see cref="HttpRequestException"/> or an HTTP
    /// 502/503/504 response. The request is rebuilt by <paramref name="requestFactory"/>
    /// on every attempt because an <see cref="HttpRequestMessage"/> can only be
    /// sent once. Backoff grows linearly. Non-transient responses are returned
    /// for the caller to inspect.
    /// </summary>
    private async Task<HttpResponseMessage> SendWithRetryAsync(
        Func<HttpRequestMessage> requestFactory,
        CancellationToken cancellationToken)
    {
        HttpRequestException? lastError = null;
        for (int attempt = 0; attempt <= _maxRetries; attempt++)
        {
            using var request = requestFactory();
            try
            {
                var response = await _http.SendAsync(request, cancellationToken);
                if (IsRetryableStatus((int)response.StatusCode) && attempt < _maxRetries)
                {
                    response.Dispose();
                    await BackoffAsync(attempt, cancellationToken);
                    continue;
                }
                return response;
            }
            catch (HttpRequestException ex)
            {
                lastError = ex;
                if (attempt < _maxRetries)
                {
                    await BackoffAsync(attempt, cancellationToken);
                    continue;
                }
                throw;
            }
        }
        // Only reached if every attempt threw — the loop returns or rethrows otherwise.
        throw lastError ?? new HttpRequestException("Request failed after retries");
    }

    /// <summary>502/503/504 are treated as transient and safe to retry.</summary>
    private static bool IsRetryableStatus(int status) =>
        status == 502 || status == 503 || status == 504;

    private async Task BackoffAsync(int attempt, CancellationToken cancellationToken)
    {
        var delay = _retryBackoff * (attempt + 1);
        if (delay > TimeSpan.Zero)
        {
            await Task.Delay(delay, cancellationToken);
        }
    }

    private static async Task EnsureSuccessAsync(HttpResponseMessage response, CancellationToken cancellationToken)
    {
        if (!response.IsSuccessStatusCode)
        {
            var body = await response.Content.ReadAsStringAsync(cancellationToken);
            throw new LokaException(
                $"Loka returned HTTP {(int)response.StatusCode}: {body}",
                (int)response.StatusCode
            );
        }
    }
}
