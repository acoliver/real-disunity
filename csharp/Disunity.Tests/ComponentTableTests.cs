using System.Diagnostics;
using Disunity.Hosting;
using UnityEngine;

internal static class ComponentTableTests
{
    private sealed class Marker : MonoBehaviour { }

    private sealed class Other : MonoBehaviour { }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            TableIndexesByOwnerAndKey();
            RemovingDropsEveryIndex();
            KindIndexFollowsTheTable();
            HostQueriesCostTheirObject();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static Marker Owned(GameObject owner) => new() { gameObject = owner };

    private static void TableIndexesByOwnerAndKey()
    {
        var table = new ComponentTable<MonoBehaviour>();
        var a = new GameObject("table a");
        var b = new GameObject("table b");
        var first = Owned(a);
        var second = Owned(b);
        var third = Owned(a);
        table.Add(30, first);
        table.Add(10, second);
        table.Add(20, third);
        Expect.Sequence([first, third], table.Of(a), "an owner's components in the order they were added");
        Expect.Sequence([second], table.Of(b), "another owner's components");
        Expect.Equal(0, table.Of(new GameObject("table none")).Count, "an owner with no components");
        Expect.That(table.TryKeyOf(third, out var key) && key == 20, "a component's key");
        Expect.That(!table.TryKeyOf(Owned(a), out _), "a component that was never added has no key");
        Expect.That(ReferenceEquals(table[10], second) && table.ContainsKey(30) && table.Count == 3, "lookup by key");
        Expect.Throws<ArgumentException>(() => table.Add(10, Owned(b)), "a key added twice");
        Expect.Equal(3, table.Count, "a refused add changes nothing");
        Expect.Throws<InvalidOperationException>(() => table.Add(40, new Marker()), "a component without a GameObject");
        Console.WriteLine("PASS: the component table finds components by key, by owner in added order, and keys by component");
    }

    private static void RemovingDropsEveryIndex()
    {
        var table = new ComponentTable<MonoBehaviour>();
        var a = new GameObject("remove a");
        var only = Owned(a);
        var kept = Owned(a);
        table.Add(1, only);
        table.Add(2, kept);
        Expect.That(table.Remove(1) && !table.Remove(1), "a key is removed once");
        Expect.Sequence([kept], table.Of(a), "the owner no longer lists it");
        Expect.That(!table.TryKeyOf(only, out _) && !table.ContainsKey(1), "neither its key nor the component is found");
        table.Add(1, only);
        Expect.Sequence([kept, only], table.Of(a), "a component added again goes last");
        table.Clear();
        Expect.That(table.Count == 0 && table.Of(a).Count == 0 && !table.TryKeyOf(kept, out _), "clear empties every index");
        Console.WriteLine("PASS: removing a component drops it from every index of the table");
    }

    private static void KindIndexFollowsTheTable()
    {
        var table = new ComponentTable<MonoBehaviour>();
        var a = new GameObject("kind a");
        var early = Owned(a);
        table.Add(9, early);
        var other = new Other { gameObject = a };
        table.Add(8, other);
        var late = Owned(a);
        Expect.Sequence([early], table.OfKind<Marker>(), "a kind asked for after an add lists what the table held");
        var version = table.VersionOfKind<Marker>();
        table.Add(7, late);
        Expect.Sequence([early, late], table.OfKind<Marker>(), "a member added later is listed in the order the members were built");
        Expect.That(table.VersionOfKind<Marker>() != version, "adding a member changes the kind's version");
        version = table.VersionOfKind<Marker>();
        table.Add(6, new Other { gameObject = a });
        Expect.Equal(version, table.VersionOfKind<Marker>(), "adding a component of another kind leaves the version");
        Expect.That(table.Remove(9) && table.VersionOfKind<Marker>() != version, "removing a member changes the version");
        Expect.Sequence([late], table.OfKind<Marker>(), "the removed member is not listed");
        Expect.Equal(2, table.OfKind<Other>().Length, "each kind lists its own members");
        table.Clear();
        Expect.Equal(0, table.OfKind<Marker>().Length, "clear empties the kind");
        Console.WriteLine("PASS: the component table lists the components of a kind in build order and versions each kind");
    }

    private static void HostQueriesCostTheirObject()
    {
        var crowd = new List<GameObject>();
        for (var i = 0; i < 2000; i++)
        {
            var value = new GameObject("crowd " + i);
            value.AddComponent<Marker>();
            crowd.Add(value);
        }
        var target = crowd[1234];
        var other = target.AddComponent<Other>();
        Expect.That(ReferenceEquals(target.GetComponent<Other>(), other) && crowd[0].GetComponent<Other>() == null, "each object finds only its own scripts");
        Expect.Equal(3, target.GetComponents<Component>().Length, "the transform and the two scripts");
        var watch = Stopwatch.StartNew();
        for (var i = 0; i < 20000; i++) target.GetComponent<Other>();
        watch.Stop();
        Console.WriteLine($"PASS: 20000 GetComponent calls among 2000 scripted objects take {watch.ElapsedMilliseconds} ms");
        foreach (var value in crowd) UnityEngine.Object.DestroyImmediate(value);
    }
}
