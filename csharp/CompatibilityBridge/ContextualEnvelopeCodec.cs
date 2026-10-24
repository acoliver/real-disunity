using System.Globalization;
using System.Text.Json;

namespace CompatibilityBridge;

internal static class ContextualEnvelopeCodec
{
    internal const int Limit = ContextualCleanupCodec.MaxRecords;
    private const ulong Safe = 9007199254740991;
    internal static bool IsEnvelope(ReadOnlySpan<byte> bytes)
    {
        var reader = new Utf8JsonReader(bytes, new JsonReaderOptions
        {
            MaxDepth = ContextualCleanupCodec.MaxDepth,
            CommentHandling = JsonCommentHandling.Disallow
        });
        if (!reader.Read() || reader.TokenType != JsonTokenType.StartObject) return false;
        while (reader.Read() && reader.TokenType != JsonTokenType.EndObject)
        {
            if (reader.TokenType != JsonTokenType.PropertyName) throw new InvalidDataException("JSON property expected");
            var name = reader.GetString();
            if (name == "callbacks") return true;
            if (!reader.Read()) throw new InvalidDataException("JSON value missing");
            reader.Skip();
        }
        return false;
    }
    internal static ContextualEnvelope Parse(ReadOnlySpan<byte> bytes)
    {
        using var parsed = ContextualCleanupCodec.Parse(bytes);
        var records = parsed.Records.Select(value => new ContextualRecord(value.Binding, value.Component, value.Kind)).ToArray();
        return new ContextualEnvelope(parsed.RawOwnedBytes.ToArray(), parsed.ContextJson.Clone(), records);
    }
    internal static ulong Integer(JsonElement value) => value.ValueKind == JsonValueKind.String
        ? ulong.Parse(value.GetString()!, CultureInfo.InvariantCulture) : value.GetUInt64();
    internal static object Exact(ulong value) => value <= Safe ? value : value.ToString(CultureInfo.InvariantCulture);
}

internal sealed record ContextualRecord(ulong Binding, ulong Component, string Kind);
internal sealed record ContextualEnvelope(byte[] Raw, JsonElement Context, ContextualRecord[] Records)
{
    internal JsonElement Lease => Context.GetProperty("lease");
    internal ulong Batch => ContextualEnvelopeCodec.Integer(Context.GetProperty("callback_batch"));
}
