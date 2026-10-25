using System.Text.Json;
using UnityEngine;

namespace CompatibilityBridge;

internal static class PhaseContexts
{
    internal const string Profile = "native_phase_context_v1";
    internal static bool Selected(string path, ulong world, JsonElement descriptor, JsonElement manifest)
    {
        var selected = false;
        if (descriptor.GetProperty("capabilities").EnumerateArray().Any(c => c.GetString() == "native_phase_context_native_v1"))
        {
            using var status = NativeDescription.Read(path, world, status: true);
            var selectedProfile = status.RootElement.GetProperty("data").GetProperty("profile").GetString();
            selected = selectedProfile is Profile or NestedStartProfile.Name or HostIdleMutationProfile.Name;
            if ((selectedProfile == HostIdleMutationProfile.Name) != HostIdleMutationProfile.Required(manifest))
                throw BridgeProtocol.Invalid("manifest host_mutation_profile must match native selected session");
            if ((selectedProfile == NestedStartProfile.Name) != NestedStartProfile.Required(manifest))
                throw BridgeProtocol.Invalid("manifest scheduler_profile must match native selected session");
        }
        var required = manifest.TryGetProperty("requirements", out var requirements)
            && requirements.TryGetProperty("execution_profile", out var profile) && profile.GetString() == Profile;
        if (required != selected) throw BridgeProtocol.Invalid("manifest execution_profile must match native selected session");
        return selected;
    }
    internal static IDisposable Claim(string phase, JsonElement request, string field = "context")
    {
        if (NativePhaseScope.Phase is not null) throw new InvalidOperationException("strict phase dispatch is nonreentrant");
        object? context = request.TryGetProperty(field, out var value) ? value.Clone() : null;
        using var response = NativeWorld.Request("phase.claim", ("phase", phase), ("context", context));
        var data = NativeWorld.Data(response);
        if (!data.GetProperty("execute").GetBoolean()) throw new InvalidDataException("native phase claim denied execution");
        return NativePhaseScope.Enter(phase, data.GetProperty("stamp"), (JsonElement)context!);
    }
}

public sealed partial class ScriptSession
{
    private void StopOwner(ScriptState state)
    {
        try { _coroutines.Stop(state.Instance); }
        catch (Exception error)
        {
            if (_strict) _failures.Record(state.Instance, "CoroutineDispose", _phase, error);
            throw;
        }
    }
    internal bool Strict => _strict;
    private void RequireStrictScope()
    {
        if (_strict) NativePhaseScope.Require();
    }
    internal void InitializeClaimed() => InitializeCore();
    internal void CleanupClaimed()
    {
        if (NativePhaseScope.Phase != "Cleanup") throw new InvalidOperationException("missing native Cleanup scope");
        using var scope = EnterPhase("Cleanup");
        Dispose();
    }
    internal void DispatchClaimed(string phase)
    {
        if (NativePhaseScope.Phase != phase) throw new InvalidOperationException("missing native phase scope");
        if (_readiness is not null) { using var nativeScope = EnterPhase(phase); _readiness.Drain(); return; }
        if (phase is "FixedUpdate" or "Update" or "LateUpdate") { DispatchCore(phase); return; }
        var lane = phase switch
        {
            "FixedCoroutines" => "FixedUpdate", "UpdateCoroutines" => "Update", "EndOfFrame" => "LateUpdate",
            _ => throw new NotSupportedException($"strict DispatchPhase('{phase}')")
        };
        using var scope = EnterPhase(phase);
        _coroutines.Dispatch(lane, (owner, error) => _failures.Record(owner, "Coroutine", phase, error));
        Reconcile();
    }
    internal void ContactsClaimed(JsonElement request)
    {
        using var actual = NativeWorld.Request("events.read", ("clear", false));
        var events = NativeWorld.Data(actual);
        if (events.ValueKind != JsonValueKind.Array || events.GetArrayLength() > 4096)
            throw new InvalidDataException("strict contacts requires a batch of at most 4096 events");
        ContactEnvelope.Validate(request, events);
        try { foreach (var contact in events.EnumerateArray()) InvokeContactCore(contact); }
        finally { NativeWorld.Request("events.read", ("clear", true)).Dispose(); }
    }
}
