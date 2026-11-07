namespace UnityEngine;

/// <summary>
/// A name held as an integer id. The id of a name is a hash of its text (the engine's choice), so equal texts give
/// equal names; the empty text and a default name are the null name.
/// </summary>
public readonly struct PropertyName : IEquatable<PropertyName>
{
    private const uint OffsetBasis = 2166136261;
    private const uint Prime = 16777619;

    private readonly int _id;

    public PropertyName(string? name) => _id = Hash(name);
    public PropertyName(PropertyName other) => _id = other._id;
    public PropertyName(int id) => _id = id;

    public static PropertyName None => default;

    public bool IsNull() => _id == 0;

    public bool Equals(PropertyName other) => _id == other._id;
    public override bool Equals(object? obj) => obj is PropertyName other && Equals(other);
    public override int GetHashCode() => _id;
    public override string ToString() => $"Unknown:{_id}";

    public static bool operator ==(PropertyName left, PropertyName right) => left._id == right._id;
    public static bool operator !=(PropertyName left, PropertyName right) => left._id != right._id;
    public static implicit operator PropertyName(string? name) => new(name);
    public static implicit operator PropertyName(int id) => new(id);

    private static int Hash(string? name)
    {
        if (string.IsNullOrEmpty(name)) return 0;
        var hash = OffsetBasis;
        foreach (var character in name)
            hash = unchecked((hash ^ character) * Prime);
        return unchecked((int)hash);
    }
}

/// <summary>A table that resolves the named references of an object, such as a playable director's scene bindings.</summary>
public interface IExposedPropertyTable
{
    void SetReferenceValue(PropertyName id, UnityEngine.Object value);
    UnityEngine.Object GetReferenceValue(PropertyName id, out bool idValid);
    void ClearReferenceValue(PropertyName id);
}

/// <summary>A reference to an object by name, resolved through a table when it holds the name and else to its default.</summary>
public struct ExposedReference<T> where T : UnityEngine.Object
{
    public PropertyName exposedName;
    public UnityEngine.Object? defaultValue;

    /// <summary>The table's object for <see cref="exposedName"/> when the table holds the name, as a <typeparamref name="T"/>
    /// (null for an object of another class); otherwise <see cref="defaultValue"/> as a <typeparamref name="T"/>.</summary>
    public readonly T? Resolve(IExposedPropertyTable? resolver)
    {
        if (resolver is not null)
        {
            var value = resolver.GetReferenceValue(exposedName, out var valid);
            if (valid) return value as T;
        }
        return defaultValue as T;
    }
}
