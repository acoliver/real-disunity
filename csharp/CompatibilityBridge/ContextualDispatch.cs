using System.Text.Json;
using UnityEngine;

namespace CompatibilityBridge;

public static unsafe partial class ManagedBridge
{
    private static int _contextualThread;
    private static bool _contextualDispatch;
    private static bool TryContextualDispatch(byte* pointer, nuint length)
    {
        if (length > BridgeProtocol.RequestLimit) throw new ArgumentOutOfRangeException(nameof(length));
        if (pointer is null && length != 0) throw new ArgumentNullException(nameof(pointer));
        var bytes = new ReadOnlySpan<byte>(pointer, (int)length);
        if (_session?.Contextual != true && !ContextualEnvelopeCodec.IsEnvelope(bytes)) return false;
        if (_contextualDispatch || NativePhaseScope.Phase is not null) throw new InvalidOperationException("contextual dispatch is nonreentrant");
        if (_contextualThread != 0 && _contextualThread != Environment.CurrentManagedThreadId)
            throw new InvalidOperationException("contextual dispatch requires owner thread");
        var envelope = ContextualEnvelopeCodec.Parse(bytes);
        if (!_cleanupPending) EnsureLoaded();
        if (_session?.Contextual != true) throw new InvalidOperationException("contextual Cleanup profile is not selected");
        _session.ValidateContextual(envelope);
        _contextualDispatch = true;
        try { _session.DispatchContextual(envelope); }
        finally { _contextualDispatch = false; }
        return true;
    }
}

public sealed partial class ScriptSession
{
    private readonly bool _contextual;
    private readonly Dictionary<ulong, ulong> _contextualBindings = new();
    private bool _contextualInitializing;
    internal bool Contextual => _contextual;
    internal bool RequiresContextualCleanup => _contextual && _initialized && !_cleanupComplete;
    private static JsonDocument ContextRequest(JsonElement lease, string op, params (string key, object? value)[] fields) =>
        NativeWorld.Request(op, fields.Append(("context", (object?)LegacyLease(lease))).ToArray());
    private static object LegacyLease(JsonElement lease) => new
    {
        world = ContextualEnvelopeCodec.Integer(lease.GetProperty("world")), session = lease.GetProperty("session").GetString(),
        frame = ContextualEnvelopeCodec.Integer(lease.GetProperty("frame")), fixed_tick = ContextualEnvelopeCodec.Integer(lease.GetProperty("fixed_tick")),
        phase = "Cleanup", sequence = ContextualEnvelopeCodec.Integer(lease.GetProperty("sequence")), token = lease.GetProperty("token").GetString()
    };
    private void ReconcileContextual()
    {
        if (_contextualInitializing || NativePhaseScope.Phase == "Cleanup") return;
        if (NativePhaseScope.Phase != "Initialize") throw new NotSupportedException("contextual profile supports serial Initialize and bulk Cleanup only");
        _contextualInitializing = true;
        try
        {
            foreach (var state in _scripts.Values.ToArray()) RegisterContextual(state.Instance);
            while (true)
            {
                JsonElement decision;
                using (var response = NativePhaseScope.Readiness("readiness.next"))
                    decision = NativeWorld.Data(response).Clone();
                if (decision.ValueKind == JsonValueKind.Null) break;
                var kind = decision.GetProperty("kind").GetString()!;
                if (kind is not ("Awake" or "OnEnable")) throw new InvalidDataException("unsupported contextual Initialize decision");
                var component = decision.GetProperty("component").GetUInt64();
                var ticket = decision.GetProperty("ticket").GetUInt64();
                bool execute;
                using (var entered = NativePhaseScope.Readiness("readiness.enter", ("ticket", ticket)))
                    execute = NativeWorld.Data(entered).GetProperty("execute").GetBoolean();
                if (execute) InvokeContextual(component, kind, ticket);
                foreach (var state in _scripts.Values.ToArray()) RegisterContextual(state.Instance);
            }
        }
        finally { _contextualInitializing = false; }
    }
    private void RegisterContextual(MonoBehaviour script)
    {
        if (_contextualBindings.ContainsKey(script.NativeHandle)) return;
        var callbacks = new[] { "Awake", "OnEnable", "Start", "FixedUpdate", "Update", "LateUpdate", "OnDisable", "OnDestroy" }
            .Where(name => ContextualMetadata.FindCallback(script.GetType(), name) is not null).ToArray();
        using var registered = NativePhaseScope.Readiness("readiness.register", ("component", script.NativeHandle), ("callbacks", callbacks));
        var binding = ContextualEnvelopeCodec.Integer(NativeWorld.Data(registered).GetProperty("binding"));
        if (binding != script.NativeHandle) throw new InvalidDataException("native contextual binding differs from component generation");
        _contextualBindings.Add(script.NativeHandle, binding);
    }
    internal void ValidateContextual(ContextualEnvelope envelope)
    {
        if (NativeWorld.AttachedWorld != ContextualEnvelopeCodec.Integer(envelope.Lease.GetProperty("world")))
            throw new InvalidDataException("contextual world mismatch");
        foreach (var record in envelope.Records)
            if (!_contextualBindings.TryGetValue(record.Component, out var binding) || binding != record.Binding
                || !_scripts.TryGetValue(record.Component, out var state)
                || ContextualMetadata.FindCallback(state.Instance.GetType(), record.Kind) is null)
                throw new InvalidDataException("contextual callback mapping or method unavailable");
        using var inspected = ContextRequest(envelope.Lease, "callbacks.inspect", ("batch", ContextualEnvelopeCodec.Exact(envelope.Batch)));
        var data = NativeWorld.Data(inspected);
        if (data.GetProperty("cursor").GetInt32() != 0 || data.GetProperty("accepted").GetBoolean() || data.GetProperty("cut").GetBoolean())
            throw new InvalidDataException("contextual batch is not reserved");
        var expected = ContextualEnvelopeCodec.Parse(System.Text.Encoding.UTF8.GetBytes(data.GetProperty("envelope").GetRawText()));
        if (!SameContext(envelope.Context, expected.Context) || !envelope.Records.SequenceEqual(expected.Records))
            throw new InvalidDataException("contextual envelope differs from native reservation");
    }
    private static bool SameContext(JsonElement actual, JsonElement expected)
    {
        foreach (var field in expected.EnumerateObject())
        {
            var value = actual.GetProperty(field.Name);
            if (field.Value.ValueKind == JsonValueKind.Object) { if (!SameContext(value, field.Value)) return false; }
            else if (field.Name is "dynamic_delta" or "fixed_delta" or "unscaled_delta")
            { if (BitConverter.SingleToInt32Bits(value.GetSingle()) != BitConverter.SingleToInt32Bits(field.Value.GetSingle())) return false; }
            else if (field.Name is "session" or "token" or "phase" or "root_phase")
            { if (value.GetString() != field.Value.GetString()) return false; }
            else if (ContextualEnvelopeCodec.Integer(value) != ContextualEnvelopeCodec.Integer(field.Value)) return false;
        }
        return true;
    }
    internal void DispatchContextual(ContextualEnvelope envelope)
    {
        using var scope = NativePhaseScope.Enter("Cleanup", envelope.Lease, JsonSerializer.SerializeToElement(LegacyLease(envelope.Lease)));
        using var phase = EnterPhase("Cleanup");
        using (var accepted = ContextualAccept.Send(envelope))
            if (!NativeWorld.Data(accepted).GetProperty("accepted").GetBoolean()) throw new InvalidDataException("native contextual acceptance denied");
        for (var index = 0; index < envelope.Records.Length; index++)
        {
            JsonElement data;
            using (var entered = NativePhaseScope.Readiness("callbacks.enter", ("batch", ContextualEnvelopeCodec.Exact(envelope.Batch)), ("index", index)))
                data = NativeWorld.Data(entered).Clone();
            if (!data.GetProperty("execute").GetBoolean())
            {
                if (!data.GetProperty("replan").GetBoolean()) throw new InvalidDataException("native contextual decision denied without replan");
                break;
            }
            var decision = data.GetProperty("decision");
            var record = envelope.Records[index];
            if (data.GetProperty("replan").GetBoolean() || decision.GetProperty("component").GetUInt64() != record.Component
                || decision.GetProperty("kind").GetString() != record.Kind || decision.GetProperty("routine").ValueKind != JsonValueKind.Null)
                throw new InvalidDataException("native entered decision mismatch; no callback retry");
            InvokeContextual(record.Component, record.Kind, decision.GetProperty("ticket").GetUInt64());
        }
        using var finished = NativePhaseScope.Readiness("callbacks.finish", ("batch", ContextualEnvelopeCodec.Exact(envelope.Batch)));
    }
    internal void FinalizeContextual()
    {
        if (_cleanupComplete) return;
        using var status = NativeWorld.Request("session.status");
        if (NativeWorld.Data(status).GetProperty("stage").GetString() != "Closed")
            throw new InvalidOperationException("contextual Cleanup incomplete: native session is not Closed");
        _contextualBindings.Clear();
        ClearBindings();
        _cleanupComplete = true;
    }
    private void InvokeContextual(ulong component, string kind, ulong ticket)
    {
        var outcome = "failed";
        try
        {
            _messages.Invoke(_scripts[component].Instance, kind,
                failed: error => System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(error).Throw());
            outcome = "ok";
        }
        catch (Exception error) { _failures.Record(_scripts[component].Instance, kind, _phase, error); }
        finally { NativePhaseScope.Readiness("readiness.complete", ("ticket", ticket), ("outcome", outcome)).Dispose(); }
    }
}
