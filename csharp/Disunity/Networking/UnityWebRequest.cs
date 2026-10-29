using System.Net;

namespace UnityEngine.Networking;

/// <summary>
/// An HTTP request. The request object model works: the address, method, headers and handlers are
/// stored and read back. The engine makes no web requests yet, so <see cref="SendWebRequest"/> throws
/// <see cref="NotSupportedException"/>, and a request stays in progress with no response. Abort on a
/// request that was never sent does nothing, which is the engine's choice.
/// </summary>
public sealed class UnityWebRequest : IDisposable
{
    public enum Result { InProgress, Success, ConnectionError, ProtocolError, DataProcessingError }

    private readonly Dictionary<string, string> _requestHeaders = new(StringComparer.OrdinalIgnoreCase);

    public UnityWebRequest(string url, string method)
    {
        uri = Parse(url);
        this.method = method;
    }

    public Uri uri { get; }
    public string method { get; }
    public bool isDone => false;
    public long responseCode => 0;
    public Result result => Result.InProgress;
    public DownloadHandler? downloadHandler { get; set; }
    public UploadHandler? uploadHandler { get; set; }

    public static UnityWebRequest Get(string uri) => new(uri, "GET") { downloadHandler = new DownloadHandlerBuffer() };

    /// <summary>Encodes as <c>application/x-www-form-urlencoded</c> does: a space becomes <c>+</c>.</summary>
    public static string EscapeURL(string s)
    {
        ArgumentNullException.ThrowIfNull(s);
        return WebUtility.UrlEncode(s);
    }

    // A rooted path such as /tmp/a parses as an absolute file URI on Linux and macOS; only a URL with a scheme counts.
    private static Uri Parse(string url) =>
        Uri.TryCreate(url, UriKind.Absolute, out var parsed) && url.Contains(':', StringComparison.Ordinal)
            ? parsed
            : throw new ArgumentException($"UnityWebRequest needs an absolute URL, got '{url}'", nameof(url));

    public Dictionary<string, string>? GetResponseHeaders() => null;

    public void SetRequestHeader(string name, string value)
    {
        if (string.IsNullOrEmpty(name)) throw new ArgumentException("a request header needs a name", nameof(name));
        _requestHeaders[name] = value;
    }

    public UnityWebRequestAsyncOperation SendWebRequest() =>
        throw new NotSupportedException("UnityWebRequest.SendWebRequest: the engine makes no web requests yet");

    public void Abort() { }

    public void Dispose()
    {
        downloadHandler?.Dispose();
        uploadHandler?.Dispose();
    }
}

/// <summary>The wait a coroutine yields on while a request runs. No request runs yet, so none is created.</summary>
public class UnityWebRequestAsyncOperation : AsyncOperation
{
    internal UnityWebRequestAsyncOperation(UnityWebRequest request) => webRequest = request;

    public UnityWebRequest webRequest { get; }
}

/// <summary>Makes the request for a texture file.</summary>
public static class UnityWebRequestTexture
{
    public static UnityWebRequest GetTexture(string uri) =>
        new(uri, "GET") { downloadHandler = new DownloadHandlerTexture(true) };
}
