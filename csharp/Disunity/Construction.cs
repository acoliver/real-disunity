using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>
/// Builds many objects whose scripts wake together, as a Unity scene load does. Objects created
/// here take components and scripts through the ordinary <c>AddComponent</c> calls, and script
/// fields can be bound freely: no <c>Awake</c> or <c>OnEnable</c> runs, and searches don't see
/// the objects, until <see cref="Activate"/>. Disposing an unactivated construction removes
/// everything it created.
/// </summary>
public sealed class Construction : IDisposable
{
    private readonly ulong _world;
    private readonly ulong _handle;
    private readonly List<GameObject> _objects = new();
    private bool _closed;

    internal Construction(ulong world, ulong handle)
    {
        _world = world;
        _handle = handle;
    }

    /// <summary>Creates an active object that stays hidden until the construction activates.</summary>
    public unsafe GameObject CreateGameObject()
    {
        ThrowIfClosed();
        ulong handle;
        DisunityHost.ThrowIfError(NativeBindings.disunity_construction_object_create(_world, _handle, &handle));
        var value = DisunityHost.Track(new GameObject(handle));
        _objects.Add(value);
        return value;
    }

    /// <summary>
    /// Publishes every object, then runs each eligible script's <c>Awake</c> followed by every
    /// <c>OnEnable</c>. <c>Start</c> runs before the next update, as in Unity.
    /// </summary>
    public unsafe void Activate()
    {
        ThrowIfClosed();
        DisunityAdvance work;
        DisunityHost.ThrowIfError(NativeBindings.disunity_construction_activate(_world, _handle, &work));
        _closed = true;
        DisunityHost.Drain(work);
    }

    /// <summary>Removes everything the construction created. Activated constructions are kept.</summary>
    public void Dispose()
    {
        if (_closed) return;
        _closed = true;
        try { DisunityHost.ThrowIfError(NativeBindings.disunity_construction_abort(_world, _handle)); }
        finally { DisunityHost.Release(_objects); }
    }

    private void ThrowIfClosed()
    {
        if (_closed) throw new InvalidOperationException("the construction was already activated or disposed");
        if (DisunityHost.WorldHandle != _world) throw new InvalidOperationException("the construction's world was unloaded");
    }
}

public static partial class DisunityHost
{
    /// <summary>Opens a construction in the loaded world.</summary>
    public static unsafe Construction BeginConstruction()
    {
        EnsureWorld();
        ulong handle;
        ThrowIfError(NativeBindings.disunity_construction_begin(WorldHandle, &handle));
        return new Construction(WorldHandle, handle);
    }

    internal static GameObject Track(GameObject value)
    {
        Tracked.Add(value);
        TrackedByHandle[value.RawHandle] = value;
        return value;
    }

    private static void RemoveDestroyedObjects()
    {
        foreach (var value in Tracked.Where(value => value.IsDestroyed)) TrackedByHandle.Remove(value.RawHandle);
        Tracked.RemoveAll(value => value.IsDestroyed);
    }

    /// <summary>
    /// Forgets objects whose native side was discarded, with their scripts and native components.
    /// </summary>
    internal static void Release(IEnumerable<GameObject> owners)
    {
        foreach (var owner in owners)
        {
            foreach (var identity in Scripts.Where(pair => ReferenceEquals(pair.Value.gameObject, owner)).Select(pair => pair.Key).ToArray())
                Scripts.Remove(identity);
            foreach (var handle in NativeComponents.Where(pair => ReferenceEquals(pair.Value.gameObject, owner)).Select(pair => pair.Key).ToArray())
                NativeComponents.Remove(handle);
            owner.MarkDestroyed();
            Tracked.Remove(owner);
            TrackedByHandle.Remove(owner.RawHandle);
        }
    }
}
