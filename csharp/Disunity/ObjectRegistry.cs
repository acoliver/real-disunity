using Disunity.Hosting;

namespace UnityEngine;

/// <summary>Finds an object by its instance ID without keeping it alive. Every object registers when it is built;
/// entries of collected objects are dropped when the table has doubled since the last sweep.</summary>
internal static class ObjectRegistry
{
    private const int FirstSweepAt = 4096;

    private static readonly Dictionary<int, WeakReference<Object>> Objects = new();
    private static int _sweepAt = FirstSweepAt;

    public static void Add(Object value)
    {
        lock (Objects)
        {
            if (Objects.Count >= _sweepAt) Sweep();
            Objects[value.GetInstanceID()] = new WeakReference<Object>(value);
        }
    }

    private static void Sweep()
    {
        foreach (var id in Objects.Where(pair => !pair.Value.TryGetTarget(out _)).Select(pair => pair.Key).ToList()) Objects.Remove(id);
        _sweepAt = Math.Max(FirstSweepAt, Objects.Count * 2);
    }

    /// <summary>The live object with this ID, or null for an unknown ID and for a destroyed object.</summary>
    public static Object? Find(int instanceId)
    {
        lock (Objects)
            return Objects.TryGetValue(instanceId, out var reference) && reference.TryGetTarget(out var value) && !value.Destroyed ? value : null;
    }

    /// <summary>Every live scene object of the world, inactive and unpublished ones too, that is a <paramref name="type"/>.</summary>
    public static Object[] SceneObjects(Type type) => DisunityHost.AllObjects(type);
}
