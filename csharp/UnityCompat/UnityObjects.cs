using System.Collections;
using System.Text.Json;

namespace UnityEngine;

[AttributeUsage(AttributeTargets.Field)] public sealed class SerializeField : Attribute;
[AttributeUsage(AttributeTargets.Class)] public sealed class CreateAssetMenuAttribute : Attribute { public string? menuName { get; set; } }
[AttributeUsage(AttributeTargets.Field)] public sealed class HeaderAttribute(string header) : Attribute { public string Header { get; } = header; }
[AttributeUsage(AttributeTargets.Field)] public sealed class RangeAttribute(float min, float max) : Attribute { public float Min { get; } = min; public float Max { get; } = max; }

public class ScriptableObject : Object
{
    public static T CreateInstance<T>() where T : ScriptableObject, new() => new();
}

public class Object
{
    public ulong NativeHandle { get; internal set; }
    internal long Epoch { get; set; } = NativeIdentity.Epoch;
    internal bool DataAsset { get; set; }
    internal string DataName { get; set; } = string.Empty;
    internal virtual ulong OwnerHandle => NativeHandle;
    public string name
    {
        get => DataAsset ? DataName : ReadObject().GetProperty("name").GetString()!;
        set { if (DataAsset) DataName = value; else NativeWorld.Request("object.set", ("object", OwnerHandle), ("name", value)).Dispose(); }
    }
    public bool IsAlive
    {
        get
        {
            if (Epoch != NativeIdentity.Epoch) return false;
            if (DataAsset) return true;
            if (NativeHandle == 0) return false;
            using var result = NativeWorld.Request(this is Component ? "component.alive" : "object.alive",
                (this is Component ? "component" : "object", NativeHandle));
            return NativeWorld.Data(result).GetBoolean();
        }
    }
    public static bool operator ==(Object? a, Object? b)
    {
        if (ReferenceEquals(a, b)) return true;
        if (a is null) return !b!.IsAlive;
        if (b is null) return !a.IsAlive;
        return !a.DataAsset && !b.DataAsset && a.Epoch == b.Epoch && a.NativeHandle == b.NativeHandle;
    }
    public static bool operator !=(Object? a, Object? b) => !(a == b);
    public static implicit operator bool(Object? value) => value is not null && value.IsAlive;
    public override bool Equals(object? other) => other is Object value && this == value;
    public override int GetHashCode() => HashCode.Combine(Epoch, NativeHandle);
    public static T Instantiate<T>(T original) where T : Object => Instantiate(original, null);
    public static T Instantiate<T>(T original, Transform? parent) where T : Object => Clone(original, parent, null, null);
    public static T Instantiate<T>(T original, Vector3 position, Quaternion rotation) where T : Object => Clone(original, null, position, rotation);
    public static T Instantiate<T>(T original, Vector3 position, Quaternion rotation, Transform? parent) where T : Object => Clone(original, parent, position, rotation);
    private static T Clone<T>(T original, Transform? parent, Vector3? position, Quaternion? rotation) where T : Object
    {
        ArgumentNullException.ThrowIfNull(original);
        using var response = NativeWorld.Request("object.clone", ("object", original.OwnerHandle), ("parent", parent?.OwnerHandle));
        var data = NativeWorld.Data(response);
        var remap = data.GetProperty("remap").EnumerateObject().ToDictionary(p => ulong.Parse(p.Name), p => p.Value.GetUInt64());
        var root = NativeIdentity.GameObject(data.GetProperty("object").GetProperty("handle").GetUInt64());
        if (position.HasValue) root.transform.position = position.Value;
        if (rotation.HasValue) root.transform.rotation = rotation.Value;
        NativeIdentity.Runtime?.Cloned(remap, original.OwnerHandle);
        return (T)(original is GameObject ? root : NativeIdentity.Resolve(remap[original.NativeHandle]));
    }
    public static void Destroy(Object? target)
    {
        if (target is null || !target.IsAlive) return;
        NativeIdentity.Runtime?.Destroying(target);
        NativeWorld.Request(target is Component ? "component.remove" : "object.destroy",
            (target is Component ? "component" : "object", target.NativeHandle)).Dispose();
    }
    public static T? FindObjectOfType<T>() where T : Object => FindObjectsByType<T>(FindObjectsSortMode.None).FirstOrDefault();
    public static T? FindFirstObjectByType<T>() where T : Object => FindObjectOfType<T>();
    public static T[] FindObjectsOfType<T>() where T : Object => FindObjectsByType<T>(FindObjectsSortMode.None);
    public static T[] FindObjectsByType<T>(FindObjectsSortMode sortMode) where T : Object
    {
        using var result = NativeWorld.Request("object.find");
        var values = NativeWorld.Data(result).EnumerateArray().Select(v => NativeIdentity.GameObject(v.GetProperty("handle").GetUInt64()));
        IEnumerable<T> found = typeof(T) == typeof(GameObject) ? values.Cast<T>() : values.SelectMany(v => v.GetComponents<Component>()).OfType<T>();
        return (sortMode == FindObjectsSortMode.InstanceID ? found.OrderBy(v => v.NativeHandle) : found).ToArray();
    }
    internal JsonElement ReadObject()
    {
        using var response = NativeWorld.Request("object.get", ("object", OwnerHandle));
        return NativeWorld.Data(response).Clone();
    }
}
public enum FindObjectsSortMode { None, InstanceID }

public class GameObject : Object
{
    internal GameObject(ulong handle) { NativeHandle = handle; }
    public GameObject() : this("New GameObject") { }
    public GameObject(string name)
    {
        using var response = NativeWorld.Request("object.create", ("name", name));
        NativeHandle = NativeWorld.Data(response).GetProperty("handle").GetUInt64();
        NativeIdentity.Register(this);
    }
    public Transform transform => GetComponent<Transform>()!;
    public string tag { get => ReadObject().GetProperty("tag").GetString()!; set => NativeWorld.Request("object.set", ("object", NativeHandle), ("tag", value)).Dispose(); }
    public bool CompareTag(string value) => tag == value;
    public bool activeSelf { get => ReadObject().GetProperty("active").GetBoolean(); set => SetActive(value); }
    public bool activeInHierarchy => ReadObject().GetProperty("active_in_hierarchy").GetBoolean();
    public void SetActive(bool value)
    {
        NativeWorld.Request("object.set", ("object", NativeHandle), ("active", value)).Dispose();
        NativeIdentity.Runtime?.Reconcile();
    }
    public T AddComponent<T>() where T : Component, new()
    {
        var fields = typeof(MonoBehaviour).IsAssignableFrom(typeof(T)) ? new Dictionary<string, object?> { ["type"] = typeof(T).FullName } : null;
        using var result = NativeWorld.Request("component.add", ("object", NativeHandle), ("kind", NativeIdentity.Kind(typeof(T))), ("fields", fields ?? new()));
        var component = new T();
        NativeIdentity.Register(component, NativeWorld.Data(result), this);
        NativeIdentity.Runtime?.Added(component);
        return component;
    }
    public T? GetComponent<T>() where T : class => GetComponents<Component>().OfType<T>().FirstOrDefault();
    public T[] GetComponents<T>() where T : class
    {
        using var result = NativeWorld.Request("component.list", ("object", NativeHandle));
        return NativeWorld.Data(result).EnumerateArray().Select(v => NativeIdentity.Component(v, this)).OfType<T>().ToArray();
    }
    public static GameObject? Find(string name) => FindNative("name", name);
    public static GameObject? FindGameObjectWithTag(string tag) => FindNative("tag", tag);
    private static GameObject? FindNative(string field, string value)
    {
        using var result = NativeWorld.Request("object.find", (field, value));
        var data = NativeWorld.Data(result);
        return data.GetArrayLength() == 0 ? null : NativeIdentity.GameObject(data[0].GetProperty("handle").GetUInt64());
    }
}

public class Component : Object
{
    public GameObject gameObject { get; internal set; } = null!;
    internal override ulong OwnerHandle => gameObject.NativeHandle;
    public Transform transform => gameObject.transform;
    public string tag { get => gameObject.tag; set => gameObject.tag = value; }
    public bool CompareTag(string value) => gameObject.CompareTag(value);
    public T? GetComponent<T>() where T : class => gameObject.GetComponent<T>();
    public T[] GetComponents<T>() where T : class => gameObject.GetComponents<T>();
    protected JsonElement Fields
    {
        get { using var result = NativeWorld.Request("component.get", ("component", NativeHandle)); return NativeWorld.Data(result).GetProperty("fields").Clone(); }
    }
    protected void SetField(string key, object? value) => NativeWorld.Request("component.set", ("component", NativeHandle), ("fields", new Dictionary<string, object?> { [key] = value })).Dispose();
    public bool enabled
    {
        get { using var result = NativeWorld.Request("component.get", ("component", NativeHandle)); return NativeWorld.Data(result).GetProperty("enabled").GetBoolean(); }
        set { NativeWorld.Request("component.set", ("component", NativeHandle), ("enabled", value)).Dispose(); NativeIdentity.Runtime?.Reconcile(); }
    }
}
public class Behaviour : Component { public bool isActiveAndEnabled => enabled && gameObject.activeInHierarchy; }
public class MonoBehaviour : Behaviour
{
    public Coroutine StartCoroutine(IEnumerator routine) => (NativeIdentity.Runtime ?? throw new InvalidOperationException("StartCoroutine requires a loaded script binder")).StartCoroutine(this, routine);
    public void StopCoroutine(Coroutine routine) => NativeIdentity.Runtime!.StopCoroutine(routine);
    public void StopCoroutine(IEnumerator routine) => NativeIdentity.Runtime!.StopCoroutine(this, routine);
    public void StopAllCoroutines() => NativeIdentity.Runtime!.StopAllCoroutines(this);
}

public static class Debug
{
    public static void Log(object? message) => Console.WriteLine(message);
    public static void LogWarning(object? message) => Console.Error.WriteLine(message);
    public static void LogError(object? message) => Console.Error.WriteLine(message);
}
