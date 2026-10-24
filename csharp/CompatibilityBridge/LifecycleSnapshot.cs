using UnityEngine;

namespace CompatibilityBridge;

internal sealed class LifecycleSnapshot
{
    private readonly Dictionary<ulong, State> _scripts = new();
    private readonly HashSet<ulong> _components = new();
    internal readonly record struct State(bool Active, bool Enabled);

    private readonly bool _compact;

    public LifecycleSnapshot(bool compact = false)
    {
        _compact = compact;
        Refresh();
    }

    public bool TryGet(ulong component, out State state) => _scripts.TryGetValue(component, out state);
    public bool ContainsComponent(ulong component) => _components.Contains(component);

    public void Refresh()
    {
        using var response = NativeWorld.Request(_compact ? "lifecycle.scripts" : "lifecycle.snapshot");
        var data = NativeWorld.Data(response);
        _scripts.Clear();
        _components.Clear();
        if (_compact)
        {
            foreach (var script in data.EnumerateArray())
                _scripts.Add(script[0].GetUInt64(), new State(script[1].GetBoolean(), script[2].GetBoolean()));
            return;
        }
        foreach (var component in data.GetProperty("components").EnumerateArray())
            _components.Add(component.GetProperty("handle").GetUInt64());
        foreach (var script in data.GetProperty("scripts").EnumerateArray())
            _scripts.Add(script.GetProperty("handle").GetUInt64(), new State(
                script.GetProperty("active_in_hierarchy").GetBoolean(), script.GetProperty("enabled").GetBoolean()));
    }
}
