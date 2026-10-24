using System.Text.Json;

namespace UnityEngine;

internal static class NativeByteProfile
{
    internal const string Name = "transport_byte_limits_v1";
    internal const int RequestLimit = 1 << 20;
    internal const int ReplyLimit = 8 << 20;

    internal static void Validate(NativeJsonWorld world)
    {
        using var describe = JsonDocument.Parse("""{"version":1,"op":"describe"}""");
        using var descriptor = world.Request(describe);
        if (!NativeWorld.Data(descriptor).GetProperty("capabilities").EnumerateArray().Any(c => c.GetString() == Name))
            throw new InvalidDataException("byte transport native capability is unavailable");
        using var query = JsonDocument.Parse("""{"version":1,"op":"transport.get"}""");
        using var response = world.Request(query);
        var data = NativeWorld.Data(response);
        var limits = data.GetProperty("limits");
        if (data.GetProperty("transport_profile").GetString() != Name
            || limits.GetProperty("max_request_bytes").GetInt32() != RequestLimit
            || limits.GetProperty("max_response_bytes").GetInt32() != ReplyLimit
            || limits.GetProperty("max_outstanding_bytes").GetInt32() != 16 << 20
            || data.GetProperty("outstanding_scope").GetString() != "world_generation")
            throw new InvalidDataException("byte transport native effective limits must match 1/8/16 MiB");
    }

    internal static byte[] Encode(JsonElement request) => ByteRequestCodec.Encode(request);

    internal static JsonDocument Document(Dictionary<string, object?> request)
    {
        using var stream = new BoundedJsonStream(RequestLimit);
        JsonSerializer.Serialize(stream, request);
        return JsonDocument.Parse(stream.ToArray());
    }
}

internal sealed class BoundedJsonStream(int limit) : Stream
{
    private readonly MemoryStream _buffer = new();
    internal byte[] ToArray() => _buffer.ToArray();
    public override bool CanRead => false;
    public override bool CanSeek => false;
    public override bool CanWrite => true;
    public override long Length => _buffer.Length;
    public override long Position { get => Length; set => throw new NotSupportedException(); }
    public override void Flush() { }
    public override void Write(byte[] buffer, int offset, int count) => Write(buffer.AsSpan(offset, count));
    public override void Write(ReadOnlySpan<byte> buffer)
    {
        if (buffer.Length > limit - Length) throw new InvalidDataException("byte transport request exceeds one MiB");
        _buffer.Write(buffer);
    }
    public override void WriteByte(byte value)
    {
        if (Length == limit) throw new InvalidDataException("byte transport request exceeds one MiB");
        _buffer.WriteByte(value);
    }
    public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
    public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
    public override void SetLength(long value) => throw new NotSupportedException();
    protected override void Dispose(bool disposing) { if (disposing) _buffer.Dispose(); base.Dispose(disposing); }
}
