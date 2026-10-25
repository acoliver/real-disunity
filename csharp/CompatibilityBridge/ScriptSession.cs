using System.Collections;
using System.Text.Json;
using CompatibilitySupport;
using UnityEngine;
using UObject = UnityEngine.Object;

namespace CompatibilityBridge;

public sealed partial class ScriptSession : IScriptRuntime, IDisposable
{
    private readonly FixtureAssemblies _assemblies;
    private readonly Dictionary<string, UObject> _stable = new(StringComparer.Ordinal);
    private readonly Dictionary<ulong, ScriptState> _scripts = new();
    private readonly Dictionary<ulong, bool> _templates = new();
    private readonly ScriptMessages _messages = new();
    private readonly CoroutineScheduler _coroutines = new();
    private readonly NativeReadiness? _readiness;
    private readonly List<string> _diagnostics = new();
    private readonly SerializedFields _fields;
    private readonly bool _compactLifecycle;
    private readonly bool _strict;
    private bool _initialized;
    private bool _reconciling;
    private bool _disposed;
    private bool _cleanupComplete;
    public IReadOnlyList<string> Diagnostics => _diagnostics;
    public int BindingCount => _stable.Count + _messages.BindingCount + _assemblies.BindingCount;
    public int ScriptCount => _scripts.Count;
    public int CoroutineCount => _readiness?.Count ?? _coroutines.Count;
    public ScriptSession(JsonElement payload) : this(payload, null) { }
    internal ScriptSession(JsonElement payload, Action<ScriptSession>? publish)
    {
        if (NativeIdentity.Runtime is not null) throw new InvalidOperationException("a script session is already attached");
        using var description = NativeWorld.Request("describe");
        _compactLifecycle = NativeWorld.Data(description).GetProperty("capabilities").EnumerateArray()
            .Any(capability => capability.GetString() == "lifecycle_scripts");
        if (NativeWorld.Data(description).GetProperty("capabilities").EnumerateArray().Any(c => c.GetString() == "native_phase_context_native_v1"))
        {
            using var status = NativeWorld.Request("session.status");
            _strict = NativeWorld.Data(status).GetProperty("profile").GetString() is PhaseContexts.Profile or NestedStartProfile.Name or HostIdleMutationProfile.Name;
        }
        if (_strict && NativePhaseScope.Phase != "Initialize") throw new InvalidOperationException("strict binding requires a claimed Initialize context");
        if (NativeReadinessProfile.Selected(payload, NativeWorld.Data(description)))
        {
            if (!_strict) throw BridgeProtocol.Invalid("native readiness requires a selected native phase session");
            _readiness = new NativeReadiness(
                handle => _scripts[handle].Instance,
                (handle, name) => _messages.Invoke(_scripts[handle].Instance, name,
                    failed: error => System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(error).Throw()),
                (owner, callback, error) => _failures.Record(owner, callback, _phase, error), NestedStartProfile.Required(payload));
        }
        _assemblies = new FixtureAssemblies();
        _fields = new SerializedFields(ResolveReference, payload.TryGetProperty("profile", out var profile) ? profile.Clone() : default, _diagnostics);
        publish?.Invoke(this);
        _assemblies.LoadPaths(payload.GetProperty("assemblies"));
        NativeIdentity.Runtime = this;
        SpriteRenderer.ResolveSprite = value => (Sprite)ResolveReference(value);
        try { BindGraph(payload.GetProperty("graph"), payload.GetProperty("handles")); }
        catch (Exception primary)
        {
            if (publish is null)
            {
                try { if (_readiness is not null) ClearBindings(); else Dispose(); }
                catch (Exception cleanup) { throw new AggregateException(primary, cleanup); }
            }
            throw;
        }
    }
    public Type ResolveType(string name) => _assemblies.ResolveType(name);
    public UObject Resolve(string id) => _stable.TryGetValue(id, out var value) ? value : throw new KeyNotFoundException($"unresolved imported identity '{id}'");
    public void Initialize()
    {
        RequireLegacyEntry();
        InitializeCore();
    }
    private void InitializeCore()
    {
        using var trace = CompatibilityTrace.Begin("session", "Initialize");
        if (_initialized) return;
        _initialized = true;
        Reconcile();
    }
    public void Dispatch(string phase)
    {
        RequireLegacyEntry();
        DispatchCore(phase);
    }
    private void RequireLegacyEntry()
    {
        if (_strict) throw new InvalidOperationException("strict dispatch requires a native phase context");
    }
    private void DispatchCore(string phase)
    {
        using var trace = CompatibilityTrace.Begin("phase", phase);
        using var context = EnterPhase(phase);
        ObjectDisposedException.ThrowIf(_disposed, this);
        if (phase is not ("Awake" or "OnEnable" or "Start" or "Update" or "FixedUpdate" or "LateUpdate" or "OnDisable" or "OnDestroy")) throw new NotSupportedException($"DispatchPhase('{phase}')");
        InitializeCore();
        var snapshot = new LifecycleSnapshot(compact: _compactLifecycle);
        Reconcile(snapshot);
        if (phase is "Awake" or "OnEnable") return;
        if (phase is "Start" or "Update" or "FixedUpdate" or "LateUpdate") PrepareStarts(snapshot);
        foreach (var state in _scripts.Values.ToArray())
        {
            var handle = state.Instance.NativeHandle;
            if (!_scripts.ContainsKey(handle) || !snapshot.TryGet(handle, out var native)) continue;
            if (phase == "OnDestroy") { try { Remove(state); } finally { snapshot.Refresh(); } continue; }
            if (phase == "OnDisable") { try { Disable(state); } finally { snapshot.Refresh(); } continue; }
            if (!state.Enabled || !native.Active || !native.Enabled) continue;
            if (phase is "Start" or "Update" or "FixedUpdate" or "LateUpdate") Start(state, snapshot.Refresh);
            if (phase != "Start" && snapshot.TryGet(handle, out native) && native.Active && native.Enabled)
                Invoke(state, phase, completed: snapshot.Refresh);
        }
        if (!_strict) _coroutines.Dispatch(phase);
        snapshot.Refresh();
        Reconcile(snapshot);
    }
    public void Reconcile()
    {
        RequireStrictScope();
        if (!_initialized || _reconciling || _disposed) return;
        if (_readiness is not null)
        {
            foreach (var state in _scripts.Values.ToArray()) _readiness.Register(state.Instance);
            _readiness.Drain();
            return;
        }
        Reconcile(new LifecycleSnapshot(compact: _compactLifecycle));
    }
    private void Reconcile(LifecycleSnapshot snapshot)
    {
        if (!_initialized || _reconciling || _disposed) return;
        _reconciling = true;
        try
        {
            // Refresh after every user callback, including mutations made before an exception.
            foreach (var state in _scripts.Values.ToArray())
            {
                if (!_scripts.ContainsKey(state.Instance.NativeHandle)) continue;
                if (!snapshot.TryGet(state.Instance.NativeHandle, out var native)) { Remove(state); snapshot.Refresh(); continue; }
                if (native.Active && !state.Awake)
                {
                    state.Awake = true;
                    Invoke(state, "Awake", completed: snapshot.Refresh);
                }
            }
            foreach (var state in _scripts.Values.ToArray())
            {
                if (!_scripts.ContainsKey(state.Instance.NativeHandle) || !snapshot.TryGet(state.Instance.NativeHandle, out var native)) continue;
                var active = state.Awake && native.Active && native.Enabled;
                if (active && !state.Enabled) { state.Enabled = true; Invoke(state, "OnEnable", completed: snapshot.Refresh); }
                else if (!active && state.Enabled) { Disable(state); snapshot.Refresh(); }
                if (snapshot.TryGet(state.Instance.NativeHandle, out native) && !native.Active && _coroutines.Count != 0)
                {
                    try { _coroutines.Stop(state.Instance); }
                    finally { snapshot.Refresh(); }
                }
            }
        }
        finally { _reconciling = false; }
    }
    private void PrepareStarts(LifecycleSnapshot snapshot)
    {
        var budget = 10000;
        while (true)
        {
            var pending = _scripts.Values.Where(s => !s.Started && s.Enabled && snapshot.TryGet(s.Instance.NativeHandle, out var native) && native.Active && native.Enabled).ToArray();
            if (pending.Length == 0) return;
            foreach (var state in pending)
            {
                if (--budget < 0) throw new InvalidOperationException("script Start creation budget exceeded (10000)");
                if (_scripts.ContainsKey(state.Instance.NativeHandle) && snapshot.TryGet(state.Instance.NativeHandle, out var native) && native.Active && native.Enabled)
                    Start(state, snapshot.Refresh);
            }
        }
    }
    private void Start(ScriptState state, Action completed)
    {
        if (state.Started) return;
        state.Started = true;
        if (Invoke(state, "Start", completed: completed) is not IEnumerator routine) return;
        try
        {
            StartCoroutine(state.Instance, new StartRoutine(routine,
                error => _failures.Record(state.Instance, "Start", _phase, error)));
        }
        finally { completed(); }
    }
    private void Disable(ScriptState state)
    {
        if (!state.Enabled) return;
        state.Enabled = false;
        if (_disposed && !_strict) _messages.Invoke(state.Instance, "OnDisable");
        else Invoke(state, "OnDisable");
    }
    private void Remove(ScriptState state)
    {
        _scripts.Remove(state.Instance.NativeHandle);
        var failures = new CleanupFailures();
        failures.Run(() => Disable(state));
        failures.Run(() => StopOwner(state));
        if (state.Awake) failures.Run(() =>
        {
            if (_disposed && !_strict) _messages.Invoke(state.Instance, "OnDestroy");
            else Invoke(state, "OnDestroy");
        });
        failures.ThrowIfAny();
    }
    public void Added(Component component)
    {
        RequireStrictScope();
        if (component is MonoBehaviour script) _scripts.Add(component.NativeHandle, new ScriptState(script));
        Reconcile();
    }
    public void Destroying(UObject target)
    {
        RequireStrictScope();
        if (_readiness is not null) return;
        var owners = new HashSet<ulong>();
        if (target is GameObject go) CollectOwners(go, owners);
        var failures = new CleanupFailures();
        foreach (var state in _scripts.Values.Where(s => owners.Contains(s.Instance.gameObject.NativeHandle) || s.Instance.NativeHandle == target.NativeHandle).ToArray())
            failures.Run(() => Remove(state));
        failures.ThrowIfAny();
    }
    private static void CollectOwners(GameObject owner, HashSet<ulong> result)
    {
        result.Add(owner.NativeHandle);
        for (var i = 0; i < owner.transform.childCount; i++) CollectOwners(owner.transform.GetChild(i).gameObject, result);
    }
    public Coroutine StartCoroutine(MonoBehaviour owner, IEnumerator routine)
    {
        RequireStrictScope();
        if (NativePhaseScope.Phase == "Cleanup") throw new InvalidOperationException("cleanup cannot start routines");
        return _readiness is not null ? _readiness.Start(owner, routine) : _coroutines.Start(owner, routine);
    }
    public void StopCoroutine(Coroutine routine) { RequireStrictScope(); if (_readiness is not null) _readiness.Stop(routine); else _coroutines.Stop(routine); }
    public void StopCoroutine(MonoBehaviour owner, IEnumerator routine) { RequireStrictScope(); if (_readiness is not null) _readiness.Stop(owner, routine); else _coroutines.Stop(owner, routine); }
    public void StopAllCoroutines(MonoBehaviour owner) { RequireStrictScope(); if (_readiness is not null) _readiness.Stop(owner); else _coroutines.Stop(owner); }
    public void Dispose()
    {
        if (_cleanupComplete) return;
        RequireStrictScope();
        if (_readiness is not null)
        {
            if (NativePhaseScope.Phase != "Cleanup") throw new InvalidOperationException("native readiness disposal requires Cleanup");
            _readiness.Clear();
            ClearBindings();
            _cleanupComplete = true;
            return;
        }
        _disposed = true;
        var failures = new CleanupFailures();
        foreach (var state in _scripts.Values.ToArray()) failures.Run(() => Remove(state));
        failures.Run(_coroutines.Clear);
        failures.Run(ClearBindings);
        _cleanupComplete = ScriptCount == 0 && CoroutineCount == 0 && BindingCount == 0;
        failures.ThrowIfAny();
    }
    private void ClearBindings()
    {
        _disposed = true;
        _scripts.Clear();
        _stable.Clear();
        _templates.Clear();
        _messages.Clear();
        SpriteRenderer.ResolveSprite = null;
        NativeIdentity.Reset();
        _assemblies.Dispose();
    }

}
