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
    internal static List<GameObject> ChildrenOf(ulong parent) => Tracked
        .Where(value => !value.IsDestroyed && ParentHandle(value) == parent)
        .OrderBy(value => value.SiblingKey).ToList();

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
