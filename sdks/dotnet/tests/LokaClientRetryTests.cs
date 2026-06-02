using System.Net;
using System.Text;
using Loka.Client;
using Xunit;

namespace Loka.Client.Tests;

/// <summary>
/// Tests for the .NET SDK's connection-retry behaviour, parity with the Java
/// and Go SDKs. Uses a mock <see cref="HttpMessageHandler"/> that counts calls
/// and returns scripted responses — no real server.
/// </summary>
public class LokaClientRetryTests
{
    /// <summary>A mock handler that scripts responses by attempt number (1-based).</summary>
    private sealed class MockHandler : HttpMessageHandler
    {
        private readonly Func<int, HttpResponseMessage> _responder;
        public int Calls { get; private set; }

        public MockHandler(Func<int, HttpResponseMessage> responder) => _responder = responder;

        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request, CancellationToken cancellationToken)
        {
            Calls++;
            return Task.FromResult(_responder(Calls));
        }
    }

    private static HttpResponseMessage Json(HttpStatusCode status, string body) =>
        new(status) { Content = new StringContent(body, Encoding.UTF8, "application/json") };

    private static LokaClient ClientFor(MockHandler handler, int maxRetries) =>
        new("http://localhost", new HttpClient(handler),
            maxRetries: maxRetries, retryBackoff: TimeSpan.FromMilliseconds(5));

    [Fact]
    public async Task RetriesOnServiceUnavailableThenSucceeds()
    {
        var handler = new MockHandler(call => call == 1
            ? Json(HttpStatusCode.ServiceUnavailable, "{\"error\":\"unavailable\"}")
            : Json(HttpStatusCode.OK, "{\"head\":{\"vars\":[]},\"results\":{\"bindings\":[]}}"));
        var client = ClientFor(handler, maxRetries: 2);

        var results = await client.SparqlAsync("SELECT ?s WHERE { ?s ?p ?o }");

        Assert.NotNull(results);
        Assert.Equal(2, handler.Calls); // one retry after the 503
    }

    [Fact]
    public async Task ExhaustsRetriesOnPersistent503()
    {
        var handler = new MockHandler(_ =>
            Json(HttpStatusCode.ServiceUnavailable, "{\"error\":\"unavailable\"}"));
        var client = ClientFor(handler, maxRetries: 2);

        var ex = await Assert.ThrowsAsync<LokaException>(() =>
            client.SparqlAsync("SELECT ?s WHERE { ?s ?p ?o }"));

        Assert.Equal(503, ex.StatusCode);
        Assert.Equal(3, handler.Calls); // initial + 2 retries
    }

    [Fact]
    public async Task DoesNotRetryOnClientError()
    {
        var handler = new MockHandler(_ =>
            Json(HttpStatusCode.BadRequest, "{\"error\":\"bad\"}"));
        var client = ClientFor(handler, maxRetries: 2);

        var ex = await Assert.ThrowsAsync<LokaException>(() => client.SparqlAsync("INVALID"));

        Assert.Equal(400, ex.StatusCode);
        Assert.Equal(1, handler.Calls); // 4xx is not transient
    }

    [Fact]
    public async Task RetryDisabledWhenMaxRetriesZero()
    {
        var handler = new MockHandler(_ =>
            Json(HttpStatusCode.ServiceUnavailable, "{\"error\":\"unavailable\"}"));
        var client = ClientFor(handler, maxRetries: 0);

        Assert.Equal(0, client.MaxRetries);
        await Assert.ThrowsAsync<LokaException>(() => client.SparqlAsync("SELECT ?s WHERE {}"));
        Assert.Equal(1, handler.Calls);
    }
}
