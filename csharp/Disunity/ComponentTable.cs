using System.Collections;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>
/// The host's components by native key, also indexed by owning GameObject and by component, so finding an
/// object's components or a component's key costs that object's components instead of the whole world. A
/// component's owner is fixed when it is added. Each owner's components keep the order they were added in.
/// </summary>
internal sealed class ComponentTable<T> : IEnumerable<KeyValuePair<ulong, T>> where T : Component
{
    private readonly Dictionary<ulong, T> _byKey = new();
    private readonly Dictionary<GameObject, List<T>> _byOwner = new(ReferenceEqualityComparer.Instance);
    private readonly Dictionary<T, ulong> _keyOf = new(ReferenceEqualityComparer.Instance);

    public T this[ulong key] => _byKey[key];

    public IEnumerable<T> Values => _byKey.Values;

    public int Count => _byKey.Count;

    public bool ContainsKey(ulong key) => _byKey.ContainsKey(key);

    public void Add(ulong key, T value)
    {
        var owner = value.gameObject ?? throw new InvalidOperationException($"a {value.GetType().Name} needs its GameObject before it is registered");
        _byKey.Add(key, value);
        _keyOf.Add(value, key);
        if (!_byOwner.TryGetValue(owner, out var owned)) _byOwner[owner] = owned = new List<T>();
        owned.Add(value);
    }

    public bool Remove(ulong key)
    {
        if (!_byKey.Remove(key, out var value)) return false;
        _keyOf.Remove(value);
        var owner = value.gameObject;
        if (_byOwner.TryGetValue(owner, out var owned) && owned.Remove(value) && owned.Count == 0) _byOwner.Remove(owner);
        return true;
    }

    public void Clear()
    {
        _byKey.Clear();
        _keyOf.Clear();
        _byOwner.Clear();
    }

    /// <summary>The components <paramref name="owner"/> holds, in the order they were added.</summary>
    public IReadOnlyList<T> Of(GameObject owner) => _byOwner.TryGetValue(owner, out var owned) ? owned : Array.Empty<T>();

    /// <summary>The key <paramref name="value"/> was added under, if it is in the table.</summary>
    public bool TryKeyOf(T value, out ulong key) => _keyOf.TryGetValue(value, out key);

    public IEnumerator<KeyValuePair<ulong, T>> GetEnumerator() => _byKey.GetEnumerator();

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}
