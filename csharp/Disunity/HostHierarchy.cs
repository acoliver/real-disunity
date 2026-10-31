using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static long _lastSiblingKey;

    /// <summary>A key larger than every earlier one; siblings are ordered by it.</summary>
    internal static long NextSiblingKey() => Interlocked.Increment(ref _lastSiblingKey);

    internal static unsafe ulong ParentHandle(GameObject value)
    {
        ulong handle;
        ThrowIfError(NativeBindings.disunity_transform_get_parent(WorldHandle, value.NativeHandle, &handle));
        return handle;
    }

    /// <summary>The live objects whose parent is <paramref name="parent"/> (0 for the roots), in sibling order.</summary>
    internal static List<GameObject> ChildrenOf(ulong parent) => parent == 0 ? Roots() : ChildrenBelow(parent);

    /// <summary>The roots are the objects with no parent, found by one pass over the world.</summary>
    private static List<GameObject> Roots() => Tracked
        .Where(value => !value.IsDestroyed && ParentHandle(value) == 0)
        .OrderBy(value => value.SiblingKey).ToList();

    /// <summary>The native world lists the children of one object; their order among siblings is kept here.</summary>
    private static List<GameObject> ChildrenBelow(ulong parent)
    {
        var children = new List<GameObject>();
        if (!TrackedByHandle.TryGetValue(parent, out var owner) || owner.IsDestroyed) return children;
        foreach (var handle in NativeChildren(parent))
            if (TrackedByHandle.TryGetValue(handle, out var child) && !child.IsDestroyed) children.Add(child);
        children.Sort((first, second) => first.SiblingKey.CompareTo(second.SiblingKey));
        return children;
    }

    private static unsafe ulong[] NativeChildren(ulong parent)
    {
        var handles = new ulong[8];
        while (true)
        {
            uint count;
            DisunityResult result;
            fixed (ulong* buffer = handles) result = NativeBindings.disunity_transform_children(WorldHandle, parent, buffer, (uint)handles.Length, &count);
            if (result.code == 0) return handles[..(int)count];
            if (result.code != BufferTooSmall) ThrowIfError(result);
            handles = new ulong[count];
        }
    }

    private const int BufferTooSmall = 2;

    /// <summary>Every live object's children by parent handle, each list in sibling order, from one pass over the world.</summary>
    internal static Dictionary<ulong, List<GameObject>> ChildMap()
    {
        var map = new Dictionary<ulong, List<GameObject>>();
        foreach (var value in Tracked.Where(value => !value.IsDestroyed).OrderBy(value => value.SiblingKey))
        {
            var parent = ParentHandle(value);
            if (!map.TryGetValue(parent, out var siblings)) map[parent] = siblings = new List<GameObject>();
            siblings.Add(value);
        }
        return map;
    }
}
