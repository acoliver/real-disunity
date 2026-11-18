using System.Text;
using Disunity.Hosting;
using Unity.Jobs;

namespace Unity.IO.LowLevel.Unsafe;

/// <summary>No persistent file-open API is supported. A default file is absent; closing it has no work to wait for.</summary>
public struct FileHandle
{
    public readonly JobHandle JobHandle => default;
    public readonly FileState Status => FileState.Absent;
    public readonly bool IsValid() => false;
    public readonly JobHandle Close() => default;
}

/// <summary>A host-owned read. Copies share identity and disposal. Dispose requests cancellation and releases the result;
/// native storage remains alive until any in-flight I/O returns. Complete waits without using the managed job scheduler.</summary>
public struct ReadHandle : IDisposable
{
    private readonly AsyncReadRequest? _request;
    internal ReadHandle(AsyncReadRequest request) => _request = request;
    public readonly long Identity => _request?.Identity ?? 0;
    public readonly JobHandle JobHandle => throw new NotSupportedException("file reads use ReadHandle.Complete; the job scheduler cannot represent pending native I/O");
    public readonly ReadStatus Status => IsValid() ? (ReadStatus)_request!.Call(1) : ReadStatus.Failed;
    public readonly bool IsValid() => _request?.IsValid ?? false;
    /// <summary>The terminal transfer count. In-progress requests report zero.</summary>
    public readonly long GetBytesRead() => IsValid() ? _request!.Call(2) : 0;
    public readonly string? Error => !IsValid() ? "invalid or disposed read handle" : DecodeError();
    private readonly string? DecodeError()
    {
        if ((ReadStatus)_request!.Call(1) == ReadStatus.InProgress) return null;
        var bytes = _request.Copy(3);
        return bytes.Length == 0 ? null : Encoding.UTF8.GetString(bytes);
    }
    /// <summary>Waits for a terminal state. Inspect Status and Error for success, cancellation or I/O failure.</summary>
    public readonly void Complete() => (_request ?? throw new InvalidOperationException("invalid read handle")).Call(7);
    /// <summary>Copies terminal bytes into a new managed array, including a partial transfer on failure/cancellation.</summary>
    public readonly byte[] GetData() => (_request ?? throw new InvalidOperationException("invalid read handle")).Copy(4);
    /// <summary>Best effort. A completed transfer or an I/O error wins over a late cancellation.</summary>
    public readonly void Cancel() { if (IsValid()) _request!.Call(5); }
    public readonly void Dispose() => _request?.Dispose();
}

/// <summary>Submits one bounded region to the native host, which owns the result buffer.</summary>
public static class AsyncReadManager
{
    public static ReadHandle Read(string path, long offset, int size) => new(AsyncReadBridge.Submit(path, offset, size));

    /// <summary>Borrowed raw pointers provide no asynchronous lifetime or capacity guarantee. Use the owned region overload.</summary>
    public static unsafe ReadHandle Read(string path, ReadCommand* commands, uint commandCount) =>
        throw new NotSupportedException("raw ReadCommand buffers have no asynchronous lifetime contract; use Read(path, offset, size) and GetData()");
}
