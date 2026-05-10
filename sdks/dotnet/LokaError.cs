namespace Loka.Client;

/// <summary>
/// Exception thrown when a Loka operation fails.
/// </summary>
public class LokaException : Exception
{
    /// <summary>
    /// The HTTP status code returned by the server, or null if the error is not HTTP-related.
    /// </summary>
    public int? StatusCode { get; }

    /// <summary>
    /// Create a new LokaException with a message and optional status code.
    /// </summary>
    public LokaException(string message, int? statusCode = null)
        : base(message)
    {
        StatusCode = statusCode;
    }

    /// <summary>
    /// Create a new LokaException wrapping an inner exception.
    /// </summary>
    public LokaException(string message, Exception innerException)
        : base(message, innerException)
    {
        StatusCode = null;
    }
}
