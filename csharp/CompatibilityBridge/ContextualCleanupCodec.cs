using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text.Json;

namespace CompatibilityBridge;

internal static partial class ContextualCleanupCodec
{
    internal const int MaxEnvelopeBytes = 1048576;
    internal const int MaxRecords = 4096;
    internal const int MaxDepth = 64;
    internal const int MaxNodes = 100000;

    internal static ParsedBatch Parse(ReadOnlySpan<byte> bytes)
    {
        if (bytes.IsEmpty || bytes.Length > MaxEnvelopeBytes) throw Invalid("envelope byte budget exceeded");
        try
        {
            Preflight(bytes);
            return Materialize(bytes);
        }
        catch (JsonException error) { throw new InvalidDataException("contextual Cleanup: invalid JSON", error); }
    }

    private static ParsedBatch Materialize(ReadOnlySpan<byte> bytes)
    {
        // Retain the authenticated transport bytes; JSON reserialization can change f32 bits.
        var owned = bytes.ToArray();
        var document = JsonDocument.Parse(owned, new JsonDocumentOptions { MaxDepth = MaxDepth });
        try
        {
            var root = document.RootElement;
            var context = ProjectContext(root.GetProperty("context"));
            var records = ProjectRecords(root.GetProperty("callbacks"));
            return new ParsedBatch(owned, document, context, records);
        }
        catch
        {
            document.Dispose();
            throw;
        }
    }

    private static CleanupRecord[] ProjectRecords(JsonElement callbacks)
    {
        var records = new CleanupRecord[callbacks.GetArrayLength()];
        var unique = new HashSet<(ulong Binding, ulong Component, string Kind)>();
        var index = 0;
        foreach (var item in callbacks.EnumerateArray())
        {
            var binding = Integer(item.GetProperty("binding"));
            var component = Integer(item.GetProperty("component"));
            var kind = item.GetProperty("kind").GetString()!;
            if (!unique.Add((binding, component, kind))) throw Invalid("duplicate callback tuple");
            records[index++] = new CleanupRecord(binding, component, kind);
        }
        return records;
    }

    private static CleanupContext ProjectContext(JsonElement context)
    {
        var time = context.GetProperty("time");
        return new CleanupContext
        {
            Lease = ProjectLease(context.GetProperty("lease")),
            RootPhase = "Cleanup", Phase = "destroy", Flags = 1,
            Time = new CleanupTime(Integer(time.GetProperty("numerator")), Integer(time.GetProperty("denominator"))),
            DynamicDelta = context.GetProperty("dynamic_delta").GetSingle(),
            FixedDelta = context.GetProperty("fixed_delta").GetSingle(),
            UnscaledDelta = context.GetProperty("unscaled_delta").GetSingle(),
            CallbackBatch = Integer(context.GetProperty("callback_batch"))
        };
    }

    private static CleanupLease ProjectLease(JsonElement lease) => new()
    {
        World = Integer(lease.GetProperty("world")), Session = lease.GetProperty("session").GetString()!,
        Frame = Integer(lease.GetProperty("frame")), FixedTick = Integer(lease.GetProperty("fixed_tick")),
        Phase = "Cleanup", Sequence = Integer(lease.GetProperty("sequence")), Token = lease.GetProperty("token").GetString()!
    };

    private static ulong Integer(JsonElement value) => value.ValueKind == JsonValueKind.Number
        ? value.GetUInt64() : ulong.Parse(value.GetString()!, NumberStyles.None, CultureInfo.InvariantCulture);

    private static InvalidDataException Invalid(string message) => new("contextual Cleanup: " + message);

    internal sealed class ParsedBatch : IDisposable
    {
        private readonly JsonDocument _document;
        internal ReadOnlyMemory<byte> RawOwnedBytes { get; }
        internal CleanupContext Context { get; }
        internal IReadOnlyList<CleanupRecord> Records { get; }
        internal JsonElement ContextJson => _document.RootElement.GetProperty("context");
        internal JsonElement LeaseJson => ContextJson.GetProperty("lease");

        internal ParsedBatch(byte[] raw, JsonDocument document, CleanupContext context, CleanupRecord[] records)
        {
            RawOwnedBytes = raw;
            _document = document;
            Context = context;
            Records = Array.AsReadOnly(records);
        }

        public void Dispose() => _document.Dispose();
    }
}

internal sealed record CleanupLease
{
    public required ulong World { get; init; }
    public required string Session { get; init; }
    public required ulong Frame { get; init; }
    public required ulong FixedTick { get; init; }
    public required string Phase { get; init; }
    public required ulong Sequence { get; init; }
    public required string Token { get; init; }
}

internal sealed record CleanupTime(ulong Numerator, ulong Denominator);

internal sealed record CleanupContext
{
    public required CleanupLease Lease { get; init; }
    public required string RootPhase { get; init; }
    public required string Phase { get; init; }
    public required uint Flags { get; init; }
    public required CleanupTime Time { get; init; }
    public required float DynamicDelta { get; init; }
    public required float FixedDelta { get; init; }
    public required float UnscaledDelta { get; init; }
    public required ulong CallbackBatch { get; init; }
}

internal sealed record CleanupRecord(ulong Binding, ulong Component, string Kind);
