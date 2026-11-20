using System.Collections;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>
/// The host's components by native key, also indexed by owning GameObject and by component, so finding an
/// object's components or a component's key costs that object's components instead of the whole world. A
/// component's owner is fixed when it is added. Each owner's components keep the order they were added in. A kind (a base class
/// such as Renderer or Light) that a caller asks for is indexed from then on, so finding every component of that kind costs
/// the kind's own members instead of every component, and the kind's version tells whether its members changed.
/// </summary>
internal sealed class ComponentTable<T> : IEnumerable<KeyValuePair<ulong, T>> where T : Component
{
    private readonly Dictionary<ulong, T> _byKey = new();
    private readonly Dictionary<GameObject, List<T>> _byOwner = new(ReferenceEqualityComparer.Instance);
    private readonly Dictionary<T, ulong> _keyOf = new(ReferenceEqualityComparer.Instance);
    private readonly Dictionary<Type, KindIndex> _kinds = new();

    private sealed class KindIndex
    {
        public readonly HashSet<T> Members = new(ReferenceEqualityComparer.Instance);
        public int Version;
    }

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
        foreach (var (kind, index) in _kinds)
            if (kind.IsInstanceOfType(value) && index.Members.Add(value)) index.Version++;
    }

    public bool Remove(ulong key)
    {
        if (!_byKey.Remove(key, out var value)) return false;
        _keyOf.Remove(value);
        foreach (var index in _kinds.Values)
            if (index.Members.Remove(value)) index.Version++;
        var owner = value.gameObject;
        if (_byOwner.TryGetValue(owner, out var owned) && owned.Remove(value) && owned.Count == 0) _byOwner.Remove(owner);
        return true;
    }

    public void Clear()
    {
        _byKey.Clear();
        _keyOf.Clear();
        _byOwner.Clear();
        foreach (var index in _kinds.Values)
        {
            index.Members.Clear();
            index.Version++;
        }
    }

    /// <summary>The components that are a <typeparamref name="TKind"/>, in the order they were built.</summary>
    public TKind[] OfKind<TKind>() where TKind : class, T =>
        Kind(typeof(TKind)).Members.Cast<TKind>().OrderByDescending(value => value.GetInstanceID()).ToArray();

    /// <summary>A number that changes whenever a <typeparamref name="TKind"/> is added to or removed from the table.</summary>
    public int VersionOfKind<TKind>() where TKind : class, T => Kind(typeof(TKind)).Version;

    private KindIndex Kind(Type kind)
    {
        if (_kinds.TryGetValue(kind, out var index)) return index;
        index = new KindIndex();
        foreach (var value in _byKey.Values)
            if (kind.IsInstanceOfType(value)) index.Members.Add(value);
        _kinds[kind] = index;
        return index;
    }

    /// <summary>The components <paramref name="owner"/> holds, in the order they were added.</summary>
    public IReadOnlyList<T> Of(GameObject owner) => _byOwner.TryGetValue(owner, out var owned) ? owned : Array.Empty<T>();

    /// <summary>The key <paramref name="value"/> was added under, if it is in the table.</summary>
    public bool TryKeyOf(T value, out ulong key) => _keyOf.TryGetValue(value, out key);

    public IEnumerator<KeyValuePair<ulong, T>> GetEnumerator() => _byKey.GetEnumerator();

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}
