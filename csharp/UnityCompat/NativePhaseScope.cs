using System.Runtime.CompilerServices;
using System.Text.Json;

[assembly: InternalsVisibleTo("CompatibilityBridge")]

namespace UnityEngine;

internal static class NativePhaseScope
{
    private static JsonElement _context;
    private static int _thread;
    internal static bool Strict { get; set; }
    internal static string? Phase { get; private set; }
    internal static ulong? Frame { get; private set; }
    internal static IDisposable Enter(string phase, JsonElement stamp, JsonElement context)
    {
        if (Phase is not null) throw new InvalidOperationException("strict phase dispatch is nonreentrant");
        _context = context.Clone();
        _thread = Environment.CurrentManagedThreadId;
        Phase = phase;
        var frame = stamp.GetProperty("frame");
        Frame = frame.ValueKind == JsonValueKind.String
            ? ulong.Parse(frame.GetString()!, System.Globalization.CultureInfo.InvariantCulture) : frame.GetUInt64();
        return new Scope();
    }
    internal static void Require()
    {
        if (Phase is null || _thread != Environment.CurrentManagedThreadId)
            throw new InvalidOperationException("strict script action requires its authenticated owner-thread phase");
    }
    internal static JsonDocument Readiness(string operation, params (string key, object? value)[] fields)
    {
        Require();
        return NativeWorld.Request(operation, fields.Append(("context", (object?)_context)).ToArray());
    }
    internal static JsonDocument Input()
    {
        if (!Strict) return NativeWorld.Request("input.get");
        if (Phase is null) throw new InvalidOperationException("strict input requires an authenticated phase");
        var projection = Phase is "FixedUpdate" or "FixedCoroutines" or "Contacts" ? "fixed" : "dynamic";
        return NativeWorld.Request("input.get", ("phase", projection));
    }
    internal static void Reset() { Strict = false; Phase = null; Frame = null; _context = default; _thread = 0; }
    private sealed class Scope : IDisposable
    {
        public void Dispose() { Phase = null; Frame = null; _context = default; _thread = 0; }
    }
}
