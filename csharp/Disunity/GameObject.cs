using System.Runtime.InteropServices;

namespace Disunity;

/// <summary>
/// A scene object owned by the native store. The C# object is a handle plus a
/// cache; the native side is authoritative and a destroyed handle makes every
/// member access throw <see cref="ObjectDisposedException"/>.
/// </summary>
public sealed class GameObject : IDisposable
{
    private readonly ulong _handle;
    private bool _destroyed;

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

    public Transform Transform { get; }

    internal bool IsDestroyed => _destroyed;

    public void Destroy()
    {
        if (_destroyed)
        {
            return;
        }
        WorldRunner.EnsureLoaded();
        var result = NativeBindings.disunity_gameobject_destroy(WorldRunner.WorldHandle, _handle);
        WorldRunner.ThrowIfError(result);
        WorldRunner.RemovePendingCommands(_handle);
        _destroyed = true;
        WorldRunner.Forget(this);
    }

    public void Dispose() => Destroy();

    internal void MarkDestroyed() => _destroyed = true;

    internal void ThrowIfDestroyed()
    {
        if (_destroyed)
        {
            throw new ObjectDisposedException(nameof(GameObject));
        }
    }
}

/// <summary>Position and Y rotation, applied through the native command buffer.</summary>
public sealed class Transform
{
    private readonly GameObject _owner;
    private Vector3 _position;
    private float _rotationY;
    private int _materialSlot;

    internal Transform(GameObject owner) => _owner = owner;

    public Vector3 position
    {
        get
        {
            _owner.ThrowIfDestroyed();
            return _position;
        }
        set
        {
            _owner.ThrowIfDestroyed();
            _position = value;
            WorldRunner.Enqueue(new NativeCommand(
                _owner.NativeHandle,
                NativeCommandKind.SetPosition,
                value.X,
                value.Y,
                value.Z));
        }
    }

    public float rotationY
    {
        get
        {
            _owner.ThrowIfDestroyed();
            return _rotationY;
        }
        set
        {
            _owner.ThrowIfDestroyed();
            _rotationY = value;
            WorldRunner.Enqueue(new NativeCommand(
                _owner.NativeHandle,
                NativeCommandKind.SetRotationY,
                value,
                0.0f,
                0.0f));
        }
    }

    /// <summary>Material slot index used by the terrain-palette renderer.</summary>
    public int materialSlot
    {
        get
        {
            _owner.ThrowIfDestroyed();
            return _materialSlot;
        }
        set
        {
            _owner.ThrowIfDestroyed();
            _materialSlot = value;
            WorldRunner.Enqueue(new NativeCommand(
                _owner.NativeHandle,
                NativeCommandKind.SetMaterialSlot,
                value,
                0.0f,
                0.0f));
        }
    }
}

/// <summary>Single-precision 3-vector mirroring the native layout.</summary>
public readonly record struct Vector3(float X, float Y, float Z);
