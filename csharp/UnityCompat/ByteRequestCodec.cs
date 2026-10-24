using System.Text.Json;
using System.Text.Json.Serialization;

namespace UnityEngine;

internal sealed class ByteRequestCodec : JsonConverter<byte[]>
{
    private static readonly JsonSerializerOptions Options = new() { Converters = { new ByteRequestCodec() } };
    internal static byte[] Encode(JsonElement request) => request.Deserialize<byte[]>(Options)!;

    public override byte[] Read(ref Utf8JsonReader reader, Type type, JsonSerializerOptions options)
    {
        using var output = new BoundedJsonStream(NativeByteProfile.RequestLimit);
        Value(ref reader, output);
        return output.ToArray();
    }

    private static void Value(ref Utf8JsonReader reader, BoundedJsonStream output)
    {
        switch (reader.TokenType)
        {
            case JsonTokenType.StartObject: Object(ref reader, output); break;
            case JsonTokenType.StartArray: Array(ref reader, output); break;
            case JsonTokenType.String: String(reader.ValueSpan, output); break;
            case JsonTokenType.Number:
            case JsonTokenType.True:
            case JsonTokenType.False:
            case JsonTokenType.Null: output.Write(reader.ValueSpan); break;
            default: throw new JsonException("invalid byte transport request token");
        }
    }

    private static void Object(ref Utf8JsonReader reader, BoundedJsonStream output)
    {
        output.WriteByte((byte)'{');
        var first = true;
        while (reader.Read() && reader.TokenType != JsonTokenType.EndObject)
        {
            if (reader.TokenType != JsonTokenType.PropertyName) throw new JsonException("expected request property");
            if (!first) output.WriteByte((byte)',');
            first = false;
            String(reader.ValueSpan, output);
            output.WriteByte((byte)':');
            if (!reader.Read()) throw new JsonException("missing request value");
            Value(ref reader, output);
        }
        output.WriteByte((byte)'}');
    }

    private static void Array(ref Utf8JsonReader reader, BoundedJsonStream output)
    {
        output.WriteByte((byte)'[');
        var first = true;
        while (reader.Read() && reader.TokenType != JsonTokenType.EndArray)
        {
            if (!first) output.WriteByte((byte)',');
            first = false;
            Value(ref reader, output);
        }
        output.WriteByte((byte)']');
    }

    private static void String(ReadOnlySpan<byte> value, BoundedJsonStream output)
    {
        output.WriteByte((byte)'"');
        output.Write(value);
        output.WriteByte((byte)'"');
    }

    public override void Write(Utf8JsonWriter writer, byte[] value, JsonSerializerOptions options) => throw new NotSupportedException();
}
