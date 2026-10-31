using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Where a component is on its way out. A pending component is still found by searches until the
/// native world retires it at the end of the frame; a hidden one (DestroyImmediate) already reads as destroyed.</summary>
internal enum ComponentLife : byte { Alive, Pending, Hidden, Retired }

public static partial class DisunityHost
{
    private static readonly List<Component> PendingComponents = new();

    /// <summary>
    /// Destroys a component. The native world keeps it until the frame ends and queues a script's OnDisable
    /// and OnDestroy for the frame's cleanup; with <paramref name="immediate"/> the component is also hidden
    /// from searches and reads as null at once, and the queued messages run before this returns. A Transform
    /// is refused with a logged error, as the engine's choice.
    /// </summary>
    internal static void DestroyComponent(Component component, bool immediate)
    {
        if (component is Transform)
        {
            Debug.LogError($"Can't destroy the Transform component of '{component.gameObject.name}'; destroy its GameObject instead.", component.gameObject);
            return;
        }
        if (component.Destroyed) return;
        if (component.Life == ComponentLife.Alive) RequestDestruction(component);
        if (!immediate) return;
        try { RunCleanupNow(); }
        finally { component.Life = ComponentLife.Hidden; }
    }

    /// <summary>
    /// Destroys an object and its descendants so they read as destroyed and drop out of searches at once; the
    /// native world retires them at the end of the frame, and their OnDisable and OnDestroy messages run
    /// before this returns.
    /// </summary>
    internal static void DestroyObjectNow(GameObject target)
    {
        if (target.IsDestroyed) return;
        var subtree = ComponentQueries.Descendants(target).ToList();
        target.Destroy();
        try { RunCleanupNow(); }
        finally
        {
            foreach (var value in subtree)
            {
                RemovePendingCommands(value.RawHandle);
                value.MarkDestroyed();
            }
        }
    }

    private static unsafe void RequestDestruction(Component component)
    {
        EnsureWorld();
        var identity = Scripts.FirstOrDefault(pair => ReferenceEquals(pair.Value, component)).Key;
        if (identity != 0) ThrowIfError(NativeBindings.disunity_script_destroy(WorldHandle, identity));
        else if (NativeComponents.ContainsKey(component.Handle)) ThrowIfError(NativeBindings.disunity_component_destroy(WorldHandle, component.Handle));
        else throw new NotSupportedException($"destroying a {component.GetType().Name} that is not attached to a GameObject");
        component.Life = ComponentLife.Pending;
        PendingComponents.Add(component);
    }

    private static unsafe void RunCleanupNow()
    {
        DisunityAdvance work;
        ThrowIfError(NativeBindings.disunity_cleanup_flush(WorldHandle, &work));
        Drain(work);
    }

    /// <summary>The native world retired every pending component with the frame; forget them here too.</summary>
    private static void RetirePendingComponents()
    {
        foreach (var component in PendingComponents) component.Life = ComponentLife.Retired;
        PendingComponents.Clear();
        foreach (var identity in Scripts.Where(pair => pair.Value.Life == ComponentLife.Retired).Select(pair => pair.Key).ToArray()) Scripts.Remove(identity);
        foreach (var handle in NativeComponents.Where(pair => pair.Value.Life == ComponentLife.Retired).Select(pair => pair.Key).ToArray()) NativeComponents.Remove(handle);
    }
}
