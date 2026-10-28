using Disunity.Hosting;

namespace UnityEngine;

[AttributeUsage(AttributeTargets.Field)]
public sealed class SerializeField : Attribute;

/// <summary>Serializes a field by reference, so it can hold a subclass or an interface's
/// implementation. Players store such values in each object's reference registry.</summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class SerializeReference : Attribute;

public enum FindObjectsSortMode { None, InstanceID }

/// <summary>
/// The base of everything Unity code can hold a reference to. A scene object (GameObject or Component) is
/// alive while its GameObject is; the native world destroys it, and destroys a ScriptableObject, when
/// Destroy asks (DestroyImmediate is not supported for them). Every other object is a managed asset, which
/// stays alive until Destroy or DestroyImmediate marks it destroyed. A destroyed object compares equal to null
/// and converts to false. Searches (FindObjectsByType and the others) find scene objects only.
/// </summary>
public partial class Object
{
    // Instance IDs count down from -2 in steps of -2 and are assigned when the object is built, as
    // Unity numbers objects created at run time (negative and even). The number is stable for the
    // object's lifetime and never reused in the process. The counter is an int, so it wraps after about a
    // billion objects (2^30), when IDs would repeat; a game does not create that many in one process.
    private static int _lastInstanceId;

    private readonly int _instanceId = Interlocked.Add(ref _lastInstanceId, -2);
    private string _name = "";
    private bool _assetDestroyed;

    public Object() => ObjectRegistry.Add(this);

    internal virtual GameObject? Owner => null;
    internal virtual bool Destroyed => _assetDestroyed;

    /// <summary>Whether Destroy or DestroyImmediate marked this object, for assets whose Destroyed also depends on other state.</summary>
    internal bool AssetDestroyed => _assetDestroyed;

    public string name
    {
        get => ReadName();
        set => WriteName(value);
    }

    /// <summary>Stored but not acted on: the engine has no editor and does not save objects.</summary>
    public HideFlags hideFlags { get; set; }

    internal virtual string ReadName() => _name;

    internal virtual void WriteName(string value)
    {
        ArgumentNullException.ThrowIfNull(value);
        _name = value;
    }

    /// <summary>Why this object cannot be destroyed, or null; built-in assets the engine shares process-wide refuse.</summary>
    internal virtual string? DestroyRefusal => null;

    internal void MarkAssetDestroyed()
    {
        if (DestroyRefusal is { } refusal) throw new InvalidOperationException(refusal);
        _assetDestroyed = true;
        OnAssetDestroyed();
    }

    /// <summary>Lets a managed asset let go of what it holds when it is destroyed.</summary>
    internal virtual void OnAssetDestroyed() { }

    /// <summary>A copy for Instantiate of an object that is not in a scene.</summary>
    internal virtual Object CloneAsset() => throw new NotSupportedException($"Object.Instantiate is not supported yet for {GetType().Name}");

    /// <summary>Marks the object as changed for the editor to save. The engine has no editor and saves no objects, so nothing happens.</summary>
    public void MarkDirty() { }

    public int GetInstanceID() => _instanceId;
    public EntityId GetEntityId() => new(_instanceId);

    public override string ToString() => Destroyed ? "null" : name + " (" + GetType().FullName + ")";

    public static void Destroy(Object? target)
    {
        if (target is null || target.Destroyed) return;
        switch (target)
        {
            case ScriptableObject asset: DisunityHost.DestroyAsset(asset); break;
            case GameObject owner: owner.Destroy(); break;
            case Component: throw new NotSupportedException("component destruction is not exposed yet");
            default: target.MarkAssetDestroyed(); break;
        }
    }

    public static void Destroy(Object? target, float delay)
    {
        if (!float.IsFinite(delay)) throw new ArgumentOutOfRangeException(nameof(delay));
        if (delay <= 0) { Destroy(target); return; }
        if (target is null || target.Destroyed) return;
        if (target is not GameObject owner) throw new NotSupportedException("delayed destruction is not supported yet for assets; it needs a scene object");
        var nanos = checked((ulong)Math.Round((double)delay * 1_000_000_000));
        DisunityHost.ThrowIfError(Disunity.Native.NativeBindings.disunity_gameobject_destroy_after(
            DisunityHost.WorldHandle, owner.NativeHandle, nanos, 1_000_000_000));
    }

    [Obsolete("Use Object.Destroy instead.")]
    public static void DestroyObject(Object? obj, float t) => Destroy(obj, t);

    [Obsolete("Use Object.Destroy instead.")]
    public static void DestroyObject(Object? obj) => Destroy(obj);

    public static void DestroyImmediate(Object? obj) => DestroyImmediate(obj, false);

    /// <summary>
    /// Destroys a managed asset at once. Scene objects and script assets are destroyed by the native
    /// world, which runs their OnDestroy messages in its next step, so destroying them immediately is
    /// not supported. The player has no asset files to protect, so <paramref name="allowDestroyingAssets"/>
    /// changes nothing.
    /// </summary>
    public static void DestroyImmediate(Object? obj, bool allowDestroyingAssets)
    {
        if (obj is null || obj.Destroyed) return;
        if (obj is GameObject or Component or ScriptableObject)
            throw new NotSupportedException("Object.DestroyImmediate is not supported yet for scene objects and script assets; use Object.Destroy");
        obj.MarkAssetDestroyed();
    }

    /// <summary>
    /// The engine runs one scene, so nothing unloads and this records the request on a root
    /// GameObject (or the root GameObject of a Component) and does nothing for assets. Unity logs a
    /// warning for an object below the root and ignores it; so does this.
    /// </summary>
    public static void DontDestroyOnLoad(Object target)
    {
        ArgumentNullException.ThrowIfNull(target);
        if (target.Owner is not { } owner || owner.IsDestroyed) return;
        if (owner.transform.parent is not null)
        {
            Debug.LogWarning("DontDestroyOnLoad only works for root GameObjects or components on root GameObjects.", target);
            return;
        }
        owner.PersistsAcrossLoads = true;
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

public abstract partial class Component : Object
{
    internal ulong Handle;
    public GameObject gameObject { get; internal set; } = null!;
    internal override GameObject Owner => gameObject;
    internal override bool Destroyed => gameObject is { } owner && owner.IsDestroyed;
    internal override string ReadName() => gameObject is { } owner ? owner.name : "";

    internal override void WriteName(string value)
    {
        if (gameObject is not { } owner) throw new InvalidOperationException("A component's name is its GameObject's, and this component is not attached to a GameObject yet");
        owner.name = value;
    }

    public Transform transform => gameObject.transform;
    public string tag { get => gameObject.tag; set => gameObject.tag = value; }
    public bool CompareTag(string value) => gameObject.CompareTag(value);
    public T? GetComponent<T>() where T : class => gameObject.GetComponent<T>();
}
