using System.Collections;
using System.Text.Json;

namespace UnityEngine;

public interface IScriptRuntime
{
    Type ResolveType(string name);
    void Reconcile();
    void Added(Component component);
    void Destroying(Object target);
    void Cloned(IReadOnlyDictionary<ulong, ulong> remap, ulong original);
    Coroutine StartCoroutine(MonoBehaviour owner, IEnumerator routine);
    void StopCoroutine(Coroutine routine);
    void StopCoroutine(MonoBehaviour owner, IEnumerator routine);
    void StopAllCoroutines(MonoBehaviour owner);
}

public static class NativeIdentity
{
    private static readonly Dictionary<ulong, WeakReference<Object>> Instances = new();
    public static int BindingCount => Instances.Count;
    public static long Epoch { get; private set; }
    public static IScriptRuntime? Runtime { get; set; }
    public static void Reset() { Instances.Clear(); Runtime = null; Epoch++; }
    public static void Register(Object value)
    {
        if (Instances.Count % 128 == 0)
            foreach (var key in Instances.Where(p => !p.Value.TryGetTarget(out _)).Select(p => p.Key).ToArray()) Instances.Remove(key);
        Instances[value.NativeHandle] = new WeakReference<Object>(value);
    }
    private static bool TryGet(ulong handle, out Object value)
    {
        value = null!;
        return Instances.TryGetValue(handle, out var reference) && reference.TryGetTarget(out value!);
    }
    public static void Register(Component value, JsonElement snapshot, GameObject owner)
    {
        value.NativeHandle = snapshot.GetProperty("handle").GetUInt64();
        value.gameObject = owner;
        Register(value);
    }
    public static GameObject GameObject(ulong handle)
    {
        if (TryGet(handle, out var value)) return (GameObject)value;
        var result = new GameObject(handle);
        Register(result);
        return result;
    }
    public static Object Resolve(ulong handle)
    {
        if (TryGet(handle, out var value)) return value;
        using var result = NativeWorld.Request("component.get", ("component", handle));
        var data = NativeWorld.Data(result);
        return Component(data, GameObject(data.GetProperty("object").GetUInt64()));
    }
    public static Component Component(JsonElement snapshot, GameObject owner)
    {
        var handle = snapshot.GetProperty("handle").GetUInt64();
        if (TryGet(handle, out var cached)) return (Component)cached;
        var kind = snapshot.GetProperty("kind").GetString()!;
        var type = kind == "Script" ? Runtime?.ResolveType(snapshot.GetProperty("fields").GetProperty("type").GetString()!)
            : typeof(Component).Assembly.GetType(kind is "Text" or "CanvasScaler" ? "UnityEngine.UI." + kind
                : kind == "TilemapCollider2D" ? "UnityEngine.Tilemaps." + kind : "UnityEngine." + kind);
        if (type is null || !typeof(Component).IsAssignableFrom(type)) throw new NotSupportedException($"Native component kind '{kind}' has no managed facade");
        var value = (Component)Activator.CreateInstance(type, nonPublic: true)!;
        Register(value, snapshot, owner);
        return value;
    }
    public static string Kind(Type type) => typeof(MonoBehaviour).IsAssignableFrom(type) ? "Script" : type.Name;
    public static void MarkData(Object value, string name) { value.DataAsset = true; value.DataName = name; }
}
