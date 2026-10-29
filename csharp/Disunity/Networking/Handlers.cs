namespace UnityEngine.Networking;

/// <summary>Receives a response body. No response arrives yet, so a handler holds no content.</summary>
public abstract class DownloadHandler : IDisposable
{
    protected DownloadHandler() { }

    public byte[]? data => null;
    public string? text => null;

    /// <summary>Called when the body is complete. It does nothing; a game's handler may override it.</summary>
    protected virtual void CompleteContent() { }

    public virtual void Dispose() { }
}

/// <summary>Collects the body as bytes.</summary>
public class DownloadHandlerBuffer : DownloadHandler
{
    public DownloadHandlerBuffer() { }
}

/// <summary>Decodes the body as a texture.</summary>
public class DownloadHandlerTexture : DownloadHandler
{
    public DownloadHandlerTexture(bool readable) { }

    public Texture2D? texture => null;
}

/// <summary>Supplies a request body.</summary>
public abstract class UploadHandler : IDisposable
{
    private protected UploadHandler(byte[] data) => this.data = data;

    public byte[] data { get; }
    public string? contentType { get; set; }

    public virtual void Dispose() { }
}

/// <summary>Sends bytes as they are. It keeps its own copy of the bytes it is given.</summary>
public class UploadHandlerRaw : UploadHandler
{
    public UploadHandlerRaw(byte[] data) : base((byte[])data.Clone()) { }
}
