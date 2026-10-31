namespace UnityEngine.SceneManagement;

/// <summary>
/// The opaque value <see cref="Scene.handle"/> returns for a scene. It wraps the engine's scene id, so two handles are
/// equal when they name the same scene, and the default handle names no scene. Handles order by their raw value.
/// <see cref="GetRawData"/> and <see cref="FromRawData"/> convert to and from that value; the engine's choice is that
/// the raw value is the scene id as an unsigned number.
/// </summary>
public readonly struct SceneHandle : IEquatable<SceneHandle>, IComparable<SceneHandle>
{
    private readonly ulong _value;

    private SceneHandle(ulong value) => _value = value;

    internal static SceneHandle Of(int sceneId) => new(unchecked((ulong)(uint)sceneId));

    public static SceneHandle FromRawData(ulong rawData) => new(rawData);

    public ulong GetRawData() => _value;

    /// <summary>The scene id the handle wraps, for code written against a numeric handle; the default handle is 0.</summary>
    public static implicit operator int(SceneHandle handle) => unchecked((int)handle._value);

    /// <summary>The handle of the scene with id <paramref name="sceneId"/>, for code that stores numeric handles; 0 is the default handle.</summary>
    public static implicit operator SceneHandle(int sceneId) => Of(sceneId);

    public static bool operator ==(SceneHandle lhs, SceneHandle rhs) => lhs._value == rhs._value;

    public static bool operator !=(SceneHandle lhs, SceneHandle rhs) => lhs._value != rhs._value;

    public bool Equals(SceneHandle other) => _value == other._value;

    public override bool Equals(object? other) => other is SceneHandle handle && Equals(handle);

    public override int GetHashCode() => _value.GetHashCode();

    public int CompareTo(SceneHandle other) => _value.CompareTo(other._value);

    public override string ToString() => _value.ToString(System.Globalization.CultureInfo.InvariantCulture);
}
