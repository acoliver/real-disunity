using System;
using System.Text.Json;

namespace CompatibilityBridge;

internal static partial class ContextualCleanupCodec
{
    private const ulong SafeInteger = 9007199254740991;

    private static void ReadValue(ref Utf8JsonReader reader, Shape shape, Field field)
    {
        switch (field)
        {
            case Field.Context: ReadObject(ref reader, Shape.Context); return;
            case Field.Lease: ReadObject(ref reader, Shape.Lease); return;
            case Field.Time: ReadObject(ref reader, Shape.Time); return;
            case Field.Payload: ReadObject(ref reader, Shape.Payload); return;
            case Field.Callbacks: ReadRecords(ref reader); return;
            case Field.DynamicDelta:
            case Field.FixedDelta:
            case Field.UnscaledDelta: ReadSingle(ref reader); return;
            case Field.Session:
            case Field.Token: Text(ref reader); return;
            case Field.RootPhase: Literal(ref reader, "Cleanup"u8); return;
            case Field.Phase: Literal(ref reader, shape == Shape.Lease ? "Cleanup"u8 : "destroy"u8); return;
            case Field.Kind: ReadKind(ref reader); return;
            default: ReadFieldInteger(ref reader, field); return;
        }
    }

    private static void ReadRecords(ref Utf8JsonReader reader)
    {
        if (reader.TokenType != JsonTokenType.StartArray) throw Invalid("callbacks array expected");
        var count = 0;
        while (reader.Read() && reader.TokenType != JsonTokenType.EndArray)
        {
            if (++count > MaxRecords) throw Invalid("callback record limit exceeded");
            ReadObject(ref reader, Shape.Record);
        }
        if (reader.TokenType != JsonTokenType.EndArray) throw Invalid("unterminated callbacks");
    }

    private static void ReadSingle(ref Utf8JsonReader reader)
    {
        if (reader.TokenType != JsonTokenType.Number || !reader.TryGetSingle(out var value) || !float.IsFinite(value))
            throw Invalid("delta must be a finite f32 number");
    }

    private static void Text(ref Utf8JsonReader reader)
    {
        if (reader.TokenType != JsonTokenType.String || reader.ValueSpan.IsEmpty)
            throw Invalid("nonempty string expected");
    }

    private static void Literal(ref Utf8JsonReader reader, ReadOnlySpan<byte> expected)
    {
        if (reader.TokenType != JsonTokenType.String || !reader.ValueTextEquals(expected))
            throw Invalid("unsupported Cleanup projection literal");
    }

    private static void ReadKind(ref Utf8JsonReader reader)
    {
        if (reader.TokenType != JsonTokenType.String
            || !(reader.ValueTextEquals("OnDisable"u8) || reader.ValueTextEquals("OnDestroy"u8)))
            throw Invalid("unsupported Cleanup callback kind");
    }

    private static void ReadFieldInteger(ref Utf8JsonReader reader, Field field)
    {
        var value = ReadInteger(ref reader);
        if (field is Field.Version or Field.Flags)
        {
            if (reader.TokenType != JsonTokenType.Number || value != 1)
                throw Invalid("version and flags must be numeric one");
            return;
        }
        if (value == 0 && field is not (Field.Frame or Field.FixedTick or Field.Numerator))
            throw Invalid("integer must be nonzero");
    }

    private static ulong ReadInteger(ref Utf8JsonReader reader)
    {
        var text = reader.ValueSpan;
        if (reader.TokenType is not (JsonTokenType.Number or JsonTokenType.String))
            throw Invalid("exact unsigned integer expected");
        if (reader.ValueIsEscaped) return ReadEscapedInteger(ref reader);
        var value = DecimalInteger(text);
        var numeric = reader.TokenType == JsonTokenType.Number;
        if (numeric != (value <= SafeInteger)) throw Invalid("noncanonical exact integer transport");
        if (numeric && !reader.TryGetUInt64(out _)) throw Invalid("unsafe numeric integer");
        return value;
    }

    private static ulong ReadEscapedInteger(ref Utf8JsonReader reader)
    {
        Span<byte> decoded = stackalloc byte[20];
        int length;
        try { length = reader.CopyString(decoded); }
        catch (ArgumentException) { throw Invalid("exact integer exceeds decimal bound"); }
        var value = DecimalInteger(decoded[..length]);
        if (value <= SafeInteger) throw Invalid("safe exact integer must be numeric");
        return value;
    }

    private static ulong DecimalInteger(ReadOnlySpan<byte> text)
    {
        if (text.IsEmpty || text.Length > 20 || (text.Length > 1 && text[0] == '0'))
            throw Invalid("noncanonical decimal integer");
        ulong value = 0;
        foreach (var b in text)
        {
            if (b < '0' || b > '9') throw Invalid("noncanonical decimal integer");
            var digit = (ulong)(b - '0');
            if (value > (ulong.MaxValue - digit) / 10) throw Invalid("unsigned integer overflow");
            value = value * 10 + digit;
        }
        return value;
    }
}
