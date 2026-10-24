using System;
using System.Buffers;
using System.Text;
using System.Text.Json;

namespace CompatibilityBridge;

internal static partial class ContextualCleanupCodec
{
    private enum Shape { Envelope, Context, Lease, Time, Record, Payload }
    private enum Field
    {
        Version, Context, Callbacks, Lease, RootPhase, Phase, Flags, Time,
        DynamicDelta, FixedDelta, UnscaledDelta, CallbackBatch, World, Session,
        Frame, FixedTick, Sequence, Token, Numerator, Denominator, Binding,
        Component, Kind, Payload
    }

    private static void Preflight(ReadOnlySpan<byte> bytes)
    {
        ValidateUtf8(bytes);
        var reader = Reader(bytes);
        var nodes = 0;
        while (reader.Read())
        {
            if (++nodes > MaxNodes) throw Invalid("JSON node budget exceeded");
            if (reader.TokenType is JsonTokenType.String or JsonTokenType.PropertyName)
                ValidateSurrogates(reader.ValueSpan);
        }
        reader = Reader(bytes);
        if (!reader.Read()) throw Invalid("JSON envelope missing");
        ReadObject(ref reader, Shape.Envelope);
        if (reader.Read()) throw Invalid("trailing JSON content");
    }

    private static Utf8JsonReader Reader(ReadOnlySpan<byte> bytes) =>
        new(bytes, new JsonReaderOptions { MaxDepth = MaxDepth, CommentHandling = JsonCommentHandling.Disallow });

    private static void ValidateUtf8(ReadOnlySpan<byte> bytes)
    {
        while (!bytes.IsEmpty)
        {
            if (Rune.DecodeFromUtf8(bytes, out _, out var consumed) != OperationStatus.Done)
                throw Invalid("invalid UTF-8");
            bytes = bytes[consumed..];
        }
    }

    private static void ValidateSurrogates(ReadOnlySpan<byte> value)
    {
        for (var i = 0; i < value.Length; i++)
        {
            if (value[i] != '\\') continue;
            if (value[++i] != 'u') continue;
            var code = HexQuad(value.Slice(i + 1, 4));
            i += 4;
            if (code is >= 0xdc00 and <= 0xdfff) throw Invalid("unpaired UTF-16 surrogate");
            if (code is < 0xd800 or > 0xdbff) continue;
            ValidateLowSurrogate(value[(i + 1)..]);
            i += 6;
        }
    }

    private static void ValidateLowSurrogate(ReadOnlySpan<byte> tail)
    {
        if (tail.Length < 6 || tail[0] != '\\' || tail[1] != 'u')
            throw Invalid("unpaired UTF-16 surrogate");
        var low = HexQuad(tail.Slice(2, 4));
        if (low is < 0xdc00 or > 0xdfff) throw Invalid("unpaired UTF-16 surrogate");
    }

    private static int HexQuad(ReadOnlySpan<byte> value)
    {
        var code = 0;
        foreach (var b in value)
        {
            var digit = b <= '9' ? b - '0' : (b | 32) - 'a' + 10;
            code = (code << 4) | digit;
        }
        return code;
    }

    private static void ReadObject(ref Utf8JsonReader reader, Shape shape)
    {
        if (reader.TokenType != JsonTokenType.StartObject) throw Invalid("JSON object expected");
        ulong seen = 0;
        var count = 0;
        while (reader.Read() && reader.TokenType != JsonTokenType.EndObject)
        {
            if (reader.TokenType != JsonTokenType.PropertyName) throw Invalid("JSON property expected");
            var field = Identify(ref reader);
            if (!Allowed(shape, field)) throw Invalid("unknown field");
            var bit = 1UL << (int)field;
            if ((seen & bit) != 0) throw Invalid("duplicate decoded field");
            seen |= bit;
            count++;
            if (!reader.Read()) throw Invalid("JSON value missing");
            ReadValue(ref reader, shape, field);
        }
        if (reader.TokenType != JsonTokenType.EndObject || count != Required(shape))
            throw Invalid("missing JSON fields");
    }

    private static Field Identify(ref Utf8JsonReader reader)
    {
        for (var index = 0; index <= (int)Field.Payload; index++)
            if (reader.ValueTextEquals(Name((Field)index))) return (Field)index;
        throw Invalid("unknown field");
    }

    private static ReadOnlySpan<byte> Name(Field field) => field switch
    {
        Field.Version => "version"u8, Field.Context => "context"u8,
        Field.Callbacks => "callbacks"u8, Field.Lease => "lease"u8,
        Field.RootPhase => "root_phase"u8, Field.Phase => "phase"u8,
        Field.Flags => "flags"u8, Field.Time => "time"u8,
        Field.DynamicDelta => "dynamic_delta"u8, Field.FixedDelta => "fixed_delta"u8,
        Field.UnscaledDelta => "unscaled_delta"u8, Field.CallbackBatch => "callback_batch"u8,
        Field.World => "world"u8, Field.Session => "session"u8, Field.Frame => "frame"u8,
        Field.FixedTick => "fixed_tick"u8, Field.Sequence => "sequence"u8,
        Field.Token => "token"u8, Field.Numerator => "numerator"u8,
        Field.Denominator => "denominator"u8, Field.Binding => "binding"u8,
        Field.Component => "component"u8, Field.Kind => "kind"u8, _ => "payload"u8
    };

    private static bool Allowed(Shape shape, Field field) => shape switch
    {
        Shape.Envelope => field is Field.Version or Field.Context or Field.Callbacks,
        Shape.Context => field is Field.Lease or Field.RootPhase or Field.Phase or Field.Flags
            or Field.Time or Field.DynamicDelta or Field.FixedDelta or Field.UnscaledDelta or Field.CallbackBatch,
        Shape.Lease => field is Field.World or Field.Session or Field.Frame or Field.FixedTick
            or Field.Phase or Field.Sequence or Field.Token,
        Shape.Time => field is Field.Numerator or Field.Denominator,
        Shape.Record => field is Field.Binding or Field.Component or Field.Kind or Field.Payload,
        _ => false
    };

    private static int Required(Shape shape) => shape switch
    {
        Shape.Envelope => 3, Shape.Context => 9, Shape.Lease => 7,
        Shape.Time => 2, Shape.Record => 4, _ => 0
    };
}
