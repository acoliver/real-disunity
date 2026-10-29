using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Networking;
using static GraphicsTestSupport;

/// <summary>The UnityWebRequest object model and its handlers. The engine makes no web requests yet.</summary>
internal static class WebRequestTests
{
    public static void Run()
    {
        Facades();
        Model();
        Headers();
        Escaping();
        Handlers();
        Disposal();
        Console.WriteLine("PASS: UnityWebRequest model, headers, EscapeURL, handlers, Dispose and SendWebRequest");
    }

    private static void Facades()
    {
        foreach (var name in new[] { "UnityEngine.Networking.UnityWebRequest", "UnityEngine.Networking.UnityWebRequest+Result",
            "UnityEngine.Networking.DownloadHandler", "UnityEngine.Networking.DownloadHandlerBuffer", "UnityEngine.Networking.DownloadHandlerTexture",
            "UnityEngine.Networking.UploadHandler", "UnityEngine.Networking.UploadHandlerRaw", "UnityEngine.Networking.UnityWebRequestTexture",
            "UnityEngine.Networking.UnityWebRequestAsyncOperation" })
            Ensure(ModuleFacades.Forwards(typeof(DisunityHost).Assembly, name), $"the module facades forward {name}");
    }

    private static void Model()
    {
        using var get = UnityWebRequest.Get("https://example.com/a?b=1");
        Ensure(get.uri == new Uri("https://example.com/a?b=1") && get.method == "GET", "Get sets uri and method");
        Ensure(!get.isDone && get.responseCode == 0 && get.result == UnityWebRequest.Result.InProgress, "defaults before a send");
        Ensure(get.GetResponseHeaders() == null, "no response headers before a response");
        Ensure(get.downloadHandler is DownloadHandlerBuffer && get.uploadHandler == null, "Get has a buffer download handler and no upload handler");
        using var post = new UnityWebRequest("https://example.com/p", "POST");
        Ensure(post.method == "POST" && post.uri.Host == "example.com", "the constructor sets uri and method");
        Ensure(post.downloadHandler == null && post.uploadHandler == null, "the constructor leaves the handlers empty");
        Throws<ArgumentException>(() => UnityWebRequest.Get("relative/path"), "a relative URL");
        Throws<ArgumentException>(() => UnityWebRequest.Get("not a url"), "an invalid URL");
        Throws<ArgumentException>(() => UnityWebRequest.Get("/rooted/path"), "a rooted path");
        Throws<ArgumentException>(() => new UnityWebRequest("", "GET"), "an empty URL");
        Throws<NotSupportedException>(() => get.SendWebRequest(), "SendWebRequest", "UnityWebRequest.SendWebRequest", "no web requests");
        get.Abort();
        Ensure(!get.isDone && get.result == UnityWebRequest.Result.InProgress, "Abort on an unsent request changes nothing");
        Ensure(Enum.GetNames<UnityWebRequest.Result>().SequenceEqual(new[] { "InProgress", "Success", "ConnectionError", "ProtocolError", "DataProcessingError" }), "Result names");
        Ensure(typeof(UnityWebRequestAsyncOperation).IsSubclassOf(typeof(AsyncOperation)), "the async operation is an AsyncOperation");
    }

    private static void Headers()
    {
        using var request = UnityWebRequest.Get("https://example.com/");
        request.SetRequestHeader("Accept", "text/plain");
        Throws<ArgumentException>(() => request.SetRequestHeader("", "x"), "an empty header name");
        Throws<ArgumentException>(() => request.SetRequestHeader(null!, "x"), "a null header name");
    }

    private static void Escaping()
    {
        Ensure(UnityWebRequest.EscapeURL("a b") == "a+b", "a space becomes a plus");
        Ensure(UnityWebRequest.EscapeURL("a&b=c/d?e#f%") == "a%26b%3Dc%2Fd%3Fe%23f%25", "reserved characters are percent-encoded");
        Ensure(UnityWebRequest.EscapeURL("") == "", "an empty string");
        Throws<ArgumentNullException>(() => UnityWebRequest.EscapeURL(null!), "a null string");
    }

    private static void Handlers()
    {
        var bytes = new byte[] { 1, 2, 3 };
        var upload = new UploadHandlerRaw(bytes);
        bytes[0] = 9;
        Ensure(upload.data.SequenceEqual(new byte[] { 1, 2, 3 }), "UploadHandlerRaw keeps a copy of the bytes");
        Ensure(upload.contentType == null, "contentType defaults to null");
        upload.contentType = "application/json";
        Ensure(upload.contentType == "application/json", "contentType is settable");
        var buffer = new DownloadHandlerBuffer();
        Ensure(buffer.data == null && buffer.text == null, "no content has arrived");
        Ensure(new DownloadHandlerTexture(true).texture == null, "no texture has arrived");
        using var request = UnityWebRequestTexture.GetTexture("https://example.com/t.png");
        Ensure(request.method == "GET" && request.downloadHandler is DownloadHandlerTexture, "GetTexture is a GET with a texture handler");
        request.uploadHandler = upload;
        request.downloadHandler = buffer;
        Ensure(request.uploadHandler == upload && request.downloadHandler == buffer, "handlers get and set");
    }

    private sealed class Tracking : DownloadHandler
    {
        public bool Disposed;
        public override void Dispose() { Disposed = true; base.Dispose(); }
        public void Complete() => CompleteContent();
    }

    private static void Disposal()
    {
        var download = new Tracking();
        download.Complete();
        var request = UnityWebRequest.Get("https://example.com/");
        request.downloadHandler = download;
        request.Dispose();
        Ensure(download.Disposed, "Dispose disposes the download handler");
        request.Dispose();
    }
}
