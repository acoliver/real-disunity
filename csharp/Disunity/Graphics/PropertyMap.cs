using System.Collections;

namespace UnityEngine;

/// <summary>The kinds of value a property store reports changes of, which the world holds.</summary>
internal enum PropertyKind { Float, Int, Vector, Matrix, Texture }

/// <summary>A dictionary of property values by ID that reports every write, so the world's copy of a material,
/// property block or the shader globals follows the managed one. Reads are those of a read-only dictionary.</summary>
internal sealed class PropertyMap<T>(Action<int> changed) : IReadOnlyDictionary<int, T>
{
    private readonly Dictionary<int, T> _values = new();

    public T this[int id]
    {
        get => _values[id];
        set
        {
            _values[id] = value;
            changed(id);
        }
    }

    public int Count => _values.Count;
    public IEnumerable<int> Keys => _values.Keys;
    public IEnumerable<T> Values => _values.Values;
    public bool ContainsKey(int id) => _values.ContainsKey(id);
    public bool TryGetValue(int id, out T value) => _values.TryGetValue(id, out value!);
    public IEnumerator<KeyValuePair<int, T>> GetEnumerator() => _values.GetEnumerator();
    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

    /// <summary>Empties the map without reporting; the owner reports a clear as a whole.</summary>
    public void Clear() => _values.Clear();
}
