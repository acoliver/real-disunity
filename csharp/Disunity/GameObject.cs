using Disunity.Native;
using Disunity.Hosting;
using System.Runtime.InteropServices;

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

    internal ulong NativeHandle
    {
        get
        {
            ThrowIfDestroyed();
            return _handle;
        }
    }

    internal override GameObject Owner => this;
    public Transform Transform { get; internal set; }
    public Transform transform => Transform;
    public T? GetComponent<T>() where T : class => DisunityHost.Components(this).OfType<T>().FirstOrDefault();
    public T AddComponent<T>() where T : Component, new() => DisunityHost.AttachComponent(this, new T());

    public string name { get => DisunityHost.ObjectText(this, 0); set => DisunityHost.ObjectText(this, 0, value); }
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
}

/// <summary>Position and Y rotation, applied through the native command buffer.</summary>
public partial class Transform : Component
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

    public unsafe void SetParent(Transform? value, bool worldPositionStays = true)
    {
        DisunityAdvance work;
        DisunityHost.ThrowIfError(NativeBindings.disunity_transform_parent(DisunityHost.WorldHandle, gameObject.NativeHandle,
            value?.gameObject.NativeHandle ?? 0, worldPositionStays ? 1u : 0u, &work));
        DisunityHost.Drain(work);
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
            unsafe { DisunityHost.ThrowIfError(NativeBindings.disunity_transform_set_position(DisunityHost.WorldHandle, gameObject.NativeHandle, value.X, value.Y, value.Z)); }
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
