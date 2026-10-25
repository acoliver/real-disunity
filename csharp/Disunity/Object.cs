using Disunity.Hosting;

namespace UnityEngine;

[AttributeUsage(AttributeTargets.Field)]
public sealed class SerializeField : Attribute;

public enum FindObjectsSortMode { None, InstanceID }

public abstract class Object
{
    internal abstract GameObject Owner { get; }
    internal virtual bool Destroyed => Owner.IsDestroyed;
    public static T Instantiate<T>(T original) where T : Object => Instantiate(original, null);
    public static T Instantiate<T>(T original, Transform? parent) where T : Object
    {
        ArgumentNullException.ThrowIfNull(original);
        return DisunityHost.Instantiate(original, parent);
    }
    public static T? FindObjectOfType<T>() where T : Object => FindFirstObjectByType<T>();
    public static T? FindFirstObjectByType<T>() where T : Object => DisunityHost.FindObjects<T>().FirstOrDefault();
    public static T[] FindObjectsByType<T>(FindObjectsSortMode sortMode) where T : Object
    {
        if (sortMode != FindObjectsSortMode.None) throw new NotSupportedException("instance ID sorting is not exposed");
        return DisunityHost.FindObjects<T>();
    }
    public static T Instantiate<T>(T original, Vector3 position, Quaternion rotation) where T : Object => Instantiate(original, position, rotation, null);
    public static T Instantiate<T>(T original, Vector3 position, Quaternion rotation, Transform? parent) where T : Object
    {
        ArgumentNullException.ThrowIfNull(original);
        return DisunityHost.Instantiate(original, parent, position, rotation);
    }
    public static void Destroy(Object? target)
    {
        if (target is null || target.Destroyed) return;
        if (target is ScriptableObject asset) { DisunityHost.DestroyAsset(asset); return; }
        if (target is not GameObject owner) throw new NotSupportedException("component destruction is not exposed yet");
        owner.Destroy();
    }
    public static implicit operator bool(Object? value) => value is not null && !value.Destroyed;
    public static bool operator ==(Object? left, Object? right)
    {
        if (ReferenceEquals(left, right)) return true;
        if (left is null) return right!.Destroyed;
        if (right is null) return left.Destroyed;
        return false;
    }
    public static bool operator !=(Object? left, Object? right) => !(left == right);
    public override bool Equals(object? other) => other is Object value && this == value;
    public override int GetHashCode() => System.Runtime.CompilerServices.RuntimeHelpers.GetHashCode(this);
}

public abstract class Component : Object
{
    internal ulong Handle;
    public GameObject gameObject { get; internal set; } = null!;
    internal override GameObject Owner => gameObject;
    public Transform transform => gameObject.transform;
    public string tag { get => gameObject.tag; set => gameObject.tag = value; }
    public bool CompareTag(string value) => gameObject.CompareTag(value);
    public T? GetComponent<T>() where T : class => gameObject.GetComponent<T>();
}
