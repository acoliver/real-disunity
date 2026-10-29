using System.Collections;
using System.Diagnostics.CodeAnalysis;
using System.Reflection;
using System.Runtime.InteropServices;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.SceneManagement;

namespace UnityEngine;

/// <summary>
/// A scene object owned by the native store. The C# object is a handle plus a
/// cache; the native side is authoritative and a destroyed handle makes every
/// member access throw <see cref="ObjectDisposedException"/>.
/// </summary>
public sealed class GameObject : Object, IDisposable
{
    private readonly ulong _handle;
    private bool _destroyed;
    private readonly ulong _world = DisunityHost.WorldHandle;

    internal GameObject(ulong handle)
    {
        _handle = handle;
        Transform = new Transform(this);
    }

    /// <summary>Orders this object among its siblings; larger keys come later.</summary>
    internal long SiblingKey { get; set; } = DisunityHost.NextSiblingKey();

    internal ulong RawHandle => _handle;

    internal ulong NativeHandle
    {
        get
        {
            ThrowIfDestroyed();
            return _handle;
        }
    }

    internal override GameObject Owner => this;
    internal override bool Destroyed => IsDestroyed;
    internal override string ReadName() => DisunityHost.ObjectText(this, 0);
    internal override void WriteName(string value) => DisunityHost.ObjectText(this, 0, value);

    /// <summary>Set by DontDestroyOnLoad. The engine runs one scene, so nothing reads it yet.</summary>
    internal bool PersistsAcrossLoads { get; set; }
    public Transform Transform { get; internal set; }
    public Transform transform => Transform;
    public T? GetComponent<T>() where T : class => DisunityHost.Components(this).OfType<T>().FirstOrDefault();
    public T AddComponent<T>() where T : Component, new() => DisunityHost.AttachComponent(this, new T());

    public string tag { get => DisunityHost.ObjectText(this, 1); set => DisunityHost.ObjectText(this, 1, value); }
    public unsafe int layer
    {
        get
        {
            uint value;
            DisunityHost.ThrowIfError(NativeBindings.disunity_object_layer_read(DisunityHost.WorldHandle, NativeHandle, &value));
            return (int)value;
        }
        set
        {
            if (value is < 0 or > 31) throw new ArgumentException("A layer is 0 to 31", nameof(value));
            DisunityHost.ThrowIfError(NativeBindings.disunity_object_layer_write(DisunityHost.WorldHandle, NativeHandle, (uint)value));
        }
    }
    public bool activeSelf => (DisunityHost.ObjectFlags(this) & 1) != 0;
    public bool activeInHierarchy => (DisunityHost.ObjectFlags(this) & 2) != 0;
    public bool CompareTag(string value) => tag == value;
    public static GameObject? FindGameObjectWithTag(string tag) => DisunityHost.ActiveObjects().FirstOrDefault(value => value.tag == tag);

    internal unsafe bool IsDestroyed
    {
        get
        {
            if (_destroyed) return true;
            if (_world != DisunityHost.WorldHandle) return _destroyed = true;
            uint alive;
            DisunityHost.ThrowIfError(NativeBindings.disunity_object_alive(_world, _handle, &alive));
            return _destroyed = alive == 0;
        }
    }

    public void Destroy()
    {
        if (IsDestroyed)
        {
            return;
        }
        DisunityHost.EnsureLoaded();
        var result = NativeBindings.disunity_gameobject_destroy(DisunityHost.WorldHandle, _handle);
        DisunityHost.ThrowIfError(result);
    }

    public unsafe void SetActive(bool value)
    {
        DisunityAdvance work;
        DisunityHost.ThrowIfError(NativeBindings.disunity_gameobject_set_active(DisunityHost.WorldHandle, NativeHandle, value ? 1u : 0u, &work));
        DisunityHost.Drain(work);
    }

    public void Dispose() => Destroy();

    internal void MarkDestroyed() => _destroyed = true;

    internal void ThrowIfDestroyed()
    {
        if (IsDestroyed)
        {
            throw new ObjectDisposedException(nameof(GameObject));
        }
    }


    /// <summary>Creates an active GameObject named "New Game Object" in the loaded world.</summary>
    public GameObject() : this("New Game Object") { }

    public GameObject(string name) : this(DisunityHost.CreateNativeObject())
    {
        DisunityHost.Track(this);
        this.name = name;
    }

    public GameObject(string name, params Type[] components) : this(name)
    {
        ArgumentNullException.ThrowIfNull(components);
        foreach (var component in components) AddComponent(component);
    }

    public GameObject gameObject => this;

    /// <summary>Stored: the engine has no static batching or lightmaps.</summary>
    public bool isStatic { get; set; }

    /// <summary>
    /// Whether the object is a scene object of the loaded scene; a prefab template of the plan belongs to none.
    /// </summary>
    public Scene scene => DisunityHost.SceneOf(this);

    /// <summary>Bit 0, the mask Unity gives the default scene, for every object in the loaded scene.</summary>
    public ulong sceneCullingMask => scene.IsValid() ? 1UL : 0UL;

    /// <summary>
    /// The active object with <paramref name="name"/>, or null. A name with '/' is a path of names from a root object.
    /// When several match, the first one created wins.
    /// </summary>
    public static GameObject? Find(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        if (!name.Contains('/')) return DisunityHost.ActiveObjects().FirstOrDefault(value => value.name == name);
        var path = name.Split('/', 2);
        var root = DisunityHost.ChildrenOf(0).FirstOrDefault(value => value.name == path[0] && value.activeInHierarchy);
        var found = path[1] == "" ? root : root?.transform.Find(path[1])?.gameObject;
        return found is { activeInHierarchy: true } ? found : null;
    }

    public static GameObject? FindWithTag(string tag) => FindGameObjectWithTag(tag);

    public static GameObject[] FindGameObjectsWithTag(string tag) => DisunityHost.ActiveObjects().Where(value => value.tag == tag).ToArray();

    /// <summary>The engine has no 3D meshes yet, so no primitive can be built.</summary>
    public static GameObject CreatePrimitive(PrimitiveType type) =>
        throw new NotSupportedException($"GameObject.CreatePrimitive({type}) is not supported yet: the engine has no 3D meshes");


    public Component? GetComponent(Type type) => ComponentQueries.On(this, type).FirstOrDefault();

    public bool TryGetComponent<T>([MaybeNullWhen(false)] out T component)
    {
        component = (T?)(object?)ComponentQueries.On(this, typeof(T)).FirstOrDefault();
        return component is not null;
    }

    public bool TryGetComponent(Type type, [MaybeNullWhen(false)] out Component component)
    {
        component = ComponentQueries.On(this, type).FirstOrDefault();
        return component is not null;
    }

    public Component AddComponent(Type componentType)
    {
        ArgumentNullException.ThrowIfNull(componentType);
        if (!typeof(Component).IsAssignableFrom(componentType) || componentType.IsAbstract)
            throw new ArgumentException($"{componentType} is not a Component class that can be added", nameof(componentType));
        return DisunityHost.AttachComponent(this, (Component)Activator.CreateInstance(componentType, nonPublic: true)!);
    }

    public Component[] GetComponents(Type type) => ComponentQueries.On(this, type).ToArray();
    public T[] GetComponents<T>() => ComponentQueries.On(this, typeof(T)).Cast<T>().ToArray();
    public void GetComponents(Type type, List<Component> results) => ComponentQueries.Fill(results, ComponentQueries.On(this, type));
    public void GetComponents<T>(List<T> results) => ComponentQueries.Fill(results, ComponentQueries.On(this, typeof(T)));

    public Component? GetComponentInChildren(Type type) => GetComponentInChildren(type, false);
    public Component? GetComponentInChildren(Type type, bool includeInactive) => ComponentQueries.InChildren(this, type, includeInactive).FirstOrDefault();
    public T? GetComponentInChildren<T>() => GetComponentInChildren<T>(false);
    public T? GetComponentInChildren<T>(bool includeInactive) => (T?)(object?)GetComponentInChildren(typeof(T), includeInactive);

    public Component? GetComponentInParent(Type type) => GetComponentInParent(type, false);
    public Component? GetComponentInParent(Type type, bool includeInactive) => ComponentQueries.InParents(this, type, includeInactive).FirstOrDefault();
    public T? GetComponentInParent<T>() => GetComponentInParent<T>(false);
    public T? GetComponentInParent<T>(bool includeInactive) => (T?)(object?)GetComponentInParent(typeof(T), includeInactive);

    public Component[] GetComponentsInChildren(Type type) => GetComponentsInChildren(type, false);
    public Component[] GetComponentsInChildren(Type type, bool includeInactive) => ComponentQueries.InChildren(this, type, includeInactive).ToArray();
    public T[] GetComponentsInChildren<T>() => GetComponentsInChildren<T>(false);
    public T[] GetComponentsInChildren<T>(bool includeInactive) => ComponentQueries.InChildren(this, typeof(T), includeInactive).Cast<T>().ToArray();
    public void GetComponentsInChildren<T>(List<T> results) => GetComponentsInChildren(false, results);
    public void GetComponentsInChildren<T>(bool includeInactive, List<T> results) =>
        ComponentQueries.Fill(results, ComponentQueries.InChildren(this, typeof(T), includeInactive));

    public Component[] GetComponentsInParent(Type type) => GetComponentsInParent(type, false);
    public Component[] GetComponentsInParent(Type type, bool includeInactive) => ComponentQueries.InParents(this, type, includeInactive).ToArray();
    public T[] GetComponentsInParent<T>() => GetComponentsInParent<T>(false);
    public T[] GetComponentsInParent<T>(bool includeInactive) => ComponentQueries.InParents(this, typeof(T), includeInactive).Cast<T>().ToArray();
    public void GetComponentsInParent<T>(bool includeInactive, List<T> results) =>
        ComponentQueries.Fill(results, ComponentQueries.InParents(this, typeof(T), includeInactive));


    private const BindingFlags AnyInstance = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;

    public void SendMessage(string methodName) => SendMessage(methodName, null, SendMessageOptions.RequireReceiver);
    public void SendMessage(string methodName, object? value) => SendMessage(methodName, value, SendMessageOptions.RequireReceiver);
    public void SendMessage(string methodName, SendMessageOptions options) => SendMessage(methodName, null, options);

    /// <summary>
    /// Calls <paramref name="methodName"/> on each MonoBehaviour of this object, public or not and inherited or not,
    /// that has a method of that name taking nothing or one parameter <paramref name="value"/> fits. An inactive object receives
    /// nothing. With RequireReceiver and no receiver an error is logged. An exception a receiver throws is logged and does not stop the others.
    /// </summary>
    public void SendMessage(string methodName, object? value, SendMessageOptions options)
    {
        ArgumentNullException.ThrowIfNull(methodName);
        var received = false;
        if (activeInHierarchy)
            foreach (var script in DisunityHost.Components(this).OfType<MonoBehaviour>().ToArray())
                received |= Deliver(script, methodName, value);
        if (!received && options == SendMessageOptions.RequireReceiver)
            Debug.LogError($"SendMessage {methodName} has no receiver!", this);
    }

    private static bool Deliver(MonoBehaviour script, string methodName, object? value)
    {
        var method = Receiver(script.GetType(), methodName, value);
        if (method is null) return false;
        try
        {
            var result = method.Invoke(script, method.GetParameters().Length == 0 ? null : [value]);
            if (result is IEnumerator routine) script.StartCoroutine(routine);
        }
        catch (TargetInvocationException error) when (error.InnerException is not null)
        {
            Debug.LogException(error.InnerException, script);
        }
        return true;
    }

    private static MethodInfo? Receiver(Type type, string methodName, object? value)
    {
        for (var current = type; current is not null; current = current.BaseType)
            foreach (var method in current.GetMethods(AnyInstance | BindingFlags.DeclaredOnly).Where(m => m.Name == methodName && !m.IsGenericMethodDefinition))
            {
                var parameters = method.GetParameters();
                if (parameters.Length == 0) return method;
                if (parameters.Length == 1 && Accepts(parameters[0].ParameterType, value)) return method;
            }
        return null;
    }

    private static bool Accepts(Type parameter, object? value) =>
        value is null ? !parameter.IsValueType || Nullable.GetUnderlyingType(parameter) is not null : parameter.IsInstanceOfType(value);
}

/// <summary>Position and Y rotation, applied through the native command buffer.</summary>
public partial class Transform : Component, System.Collections.IEnumerable
{

    internal Transform(GameObject owner) { gameObject = owner; }
    protected Transform() { }

    private unsafe DisunityObjectState Read()
    {
        DisunityObjectState state;
        DisunityHost.ThrowIfError(NativeBindings.disunity_gameobject_read(DisunityHost.WorldHandle, gameObject.NativeHandle, &state));
        return state;
    }

    public unsafe Transform? parent
    {
        get
        {
            ulong handle;
            DisunityHost.ThrowIfError(NativeBindings.disunity_transform_get_parent(DisunityHost.WorldHandle, gameObject.NativeHandle, &handle));
            return handle == 0 ? null : DisunityHost.ResolveObject(handle).transform;
        }
        set => SetParent(value);
    }

    public Vector3 position
    {
        get
        {
            gameObject.ThrowIfDestroyed();
            var state = Read();
            return new Vector3(state.x, state.y, state.z);
        }
        set
        {
            gameObject.ThrowIfDestroyed();
            unsafe { DisunityHost.ThrowIfError(NativeBindings.disunity_transform_set_position(DisunityHost.WorldHandle, gameObject.NativeHandle, value.x, value.y, value.z)); }
            MarkChanged();
        }
    }

    public float rotationY
    {
        get
        {
            gameObject.ThrowIfDestroyed();
            return Read().rotation_y_radians;
        }
        set
        {
            gameObject.ThrowIfDestroyed();
            unsafe { DisunityHost.ThrowIfError(NativeBindings.disunity_transform_set_rotation_y(DisunityHost.WorldHandle, gameObject.NativeHandle, value)); }
            MarkChanged();
        }
    }

    /// <summary>Material slot index used by the terrain-palette renderer.</summary>
    public int materialSlot
    {
        get
        {
            gameObject.ThrowIfDestroyed();
            return checked((int)Read().material_slot);
        }
        set
        {
            gameObject.ThrowIfDestroyed();
            DisunityHost.Enqueue(new NativeCommand(
                gameObject.NativeHandle,
                NativeCommandKind.SetMaterialSlot,
                value,
                0.0f,
                0.0f));
        }
    }
}
