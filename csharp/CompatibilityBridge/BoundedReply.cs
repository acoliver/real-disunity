using System.Text.Json;

namespace CompatibilityBridge;

internal sealed class BoundedReply : Stream
{
    private readonly MemoryStream _buffer = new();
    internal static byte[] Encode(object value)
    {
        using var stream = new BoundedReply();
        JsonSerializer.Serialize(stream, value);
        return stream._buffer.ToArray();
    }
    public override bool CanRead => false;
    public override bool CanSeek => false;
    public override bool CanWrite => true;
    public override long Length => _buffer.Length;
    public override long Position { get => Length; set => throw new NotSupportedException(); }
    public override void Flush() { }
    public override void Write(byte[] buffer, int offset, int count) => Write(buffer.AsSpan(offset, count));
    public override void Write(ReadOnlySpan<byte> buffer)
    {
        if (buffer.Length > BridgeProtocol.ResponseLimit - Length)
            throw new InvalidDataException("managed observation response budget exceeded");
        _buffer.Write(buffer);
    }
    public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
    public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
    public override void SetLength(long value) => throw new NotSupportedException();
    protected override void Dispose(bool disposing) { if (disposing) _buffer.Dispose(); base.Dispose(disposing); }
}
