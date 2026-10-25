using System.Collections;
using System.Text.Json;
using UnityEngine;

namespace CompatibilityBridge;

internal sealed partial class NativeReadiness(
    Func<ulong, MonoBehaviour> instance,
    Func<ulong, string, object?> invoke,
    Action<MonoBehaviour, string, Exception> failed,
    bool immediateStarts)
{
    private static readonly string[] Callbacks = ["Awake", "OnEnable", "Start", "FixedUpdate", "Update", "LateUpdate", "OnDisable", "OnDestroy"];
    private readonly HashSet<ulong> _registered = new();
    private readonly Dictionary<ulong, Routine> _routines = new();
    private bool _draining;
    private bool _entered;
    internal int Count => _routines.Values.Count(r => r.Iterator is not null);
    internal void Register(MonoBehaviour script)
    {
        if (_registered.Contains(script.NativeHandle)) return;
        var callbacks = Callbacks.Where(name => SerializedFields.FindMethod(script.GetType(), name, Type.EmptyTypes) is not null).ToArray();
        Request("register", ("component", script.NativeHandle), ("callbacks", callbacks));
        _registered.Add(script.NativeHandle);
    }
    internal void Drain()
    {
        NativePhaseScope.Require();
        if (_entered)
        {
            if (immediateStarts) while (Next() is { } child) Execute(child);
            return;
        }
        if (_draining) return;
        _draining = true;
        try
        {
            while (Next() is { } decision) Execute(decision);
            ReleaseTerminals();
        }
        finally { _draining = false; }
    }
    private static JsonElement? Next()
    {
        using var response = NativePhaseScope.Readiness("readiness.next");
        var data = NativeWorld.Data(response);
        return data.ValueKind == JsonValueKind.Null ? null : data.Clone();
    }
    private bool Execute(JsonElement decision)
    {
        var ticket = decision.GetProperty("ticket").GetUInt64();
        using (var response = NativePhaseScope.Readiness("readiness.enter", ("ticket", ticket)))
            if (!NativeWorld.Data(response).GetProperty("execute").GetBoolean()) return false;
        var component = decision.GetProperty("component").GetUInt64();
        var kind = decision.GetProperty("kind").GetString()!;
        var routine = decision.GetProperty("routine");
        var parentEntered = _entered;
        _entered = true;
        try { return ExecuteEntered(ticket, component, kind, routine); }
        finally { _entered = parentEntered; }
    }
    private bool ExecuteEntered(ulong ticket, ulong component, string kind, JsonElement routine)
    {
        if (routine.ValueKind != JsonValueKind.Null)
        {
            return ExecuteRoutine(ticket, routine.GetUInt64(), kind);
        }
        object? result = null;
        var outcome = "failed";
        try { result = invoke(component, kind); outcome = "ok"; }
        catch (Exception error) { failed(instance(component), kind, error); }
        finally { Complete(ticket, outcome); }
        if (kind == "Start" && result is IEnumerator iterator) AdmitStart(instance(component), iterator);
        return false;
    }
    private void AdmitStart(MonoBehaviour owner, IEnumerator iterator)
    {
        try { StartCore(owner, iterator); }
        catch (NotSupportedException error) { failed(owner, "Start", error); }
        catch (NativeWorldException error) when (error.Code == "invalid_request")
        {
            failed(owner, "Start", new NotSupportedException("native_readiness_single_v1 cannot admit the returned Start iterator: " + error.Message, error));
        }
    }
    private void Complete(ulong ticket, string outcome)
    {
        Request("complete", ("ticket", ticket), ("outcome", outcome));
    }
    private static void Request(string op, params (string key, object? value)[] fields) =>
        NativePhaseScope.Readiness("readiness." + op, fields).Dispose();
    internal Coroutine Start(MonoBehaviour owner, IEnumerator iterator)
    {
        NativePhaseScope.Require();
        if (_entered && !immediateStarts)
            throw new NotSupportedException("native_readiness_single_v1 cannot synchronously StartCoroutine inside an entered callback or iterator operation");
        var result = StartCore(owner, iterator);
        Drain();
        return result;
    }
    private Coroutine StartCore(MonoBehaviour owner, IEnumerator iterator)
    {
        ArgumentNullException.ThrowIfNull(iterator);
        if (_routines.Values.Any(r => ReferenceEquals(r.Iterator, iterator)))
            throw new NotSupportedException("native_readiness_single_v1 requires a distinct iterator per routine");
        ulong id;
        JsonElement? immediate;
        using (var response = NativePhaseScope.Readiness("readiness.start", ("component", owner.NativeHandle)))
        {
            var data = NativeWorld.Data(response);
            id = data.GetProperty("routine").GetUInt64();
            immediate = data.TryGetProperty("immediate", out var decision) ? decision.Clone() : null;
        }
        var result = new Coroutine(owner);
        _routines.Add(id, new Routine(result, iterator));
        if (immediate is { } child) DrainImmediate(id, child);
        return result;
    }
    internal void Stop(Coroutine coroutine)
    {
        var id = Find(coroutine);
        Request("cancel", ("routine", id));
        Drain();
    }
    internal void Stop(MonoBehaviour owner, IEnumerator? iterator = null)
    {
        foreach (var pair in _routines.Where(p => ReferenceEquals(p.Value.Coroutine.Owner, owner)
            && (iterator is null || ReferenceEquals(p.Value.Iterator, iterator))).ToArray())
            Request("cancel", ("routine", pair.Key));
        Drain();
    }
    private ulong Find(Coroutine coroutine)
    {
        foreach (var pair in _routines)
            if (ReferenceEquals(pair.Value.Coroutine, coroutine)) return pair.Key;
        throw new NotSupportedException("native_readiness_single_v1 cannot join or cancel a foreign or released routine");
    }
    private void ReleaseTerminals()
    {
        foreach (var pair in _routines.Where(p => p.Value.Iterator is null).ToArray())
        {
            if (_routines.Values.Any(r => r.Join == pair.Key)) continue;
            Request("release", ("routine", pair.Key));
            _routines.Remove(pair.Key);
        }
    }
    internal void Clear()
    {
        Drain();
        if (_routines.Count != 0) throw new InvalidOperationException("native Cleanup did not drain retained routine records");
        _registered.Clear();
    }
    private sealed class Routine(Coroutine coroutine, IEnumerator iterator)
    {
        internal Coroutine Coroutine { get; } = coroutine;
        internal IEnumerator? Iterator { get; set; } = iterator;
        internal ulong? Join { get; set; }
    }
}
