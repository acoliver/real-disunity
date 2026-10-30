using UnityEngine;
using UnityEngine.Pool;

/// <summary>The managed object pools and the UI vertex struct.</summary>
internal static class PoolTests
{
    public static void Run()
    {
        CountsAndHooks();
        Console.WriteLine("PASS: ObjectPool counts elements and runs the get, release and destroy hooks in order");
        DoubleRelease();
        Console.WriteLine("PASS: ObjectPool throws on a double release with collection checks and allows it without");
        MaxSize();
        Console.WriteLine("PASS: ObjectPool destroys a release beyond maxSize and rejects a maxSize of zero");
        ClearAndDispose();
        Console.WriteLine("PASS: ObjectPool.Clear and Dispose destroy the idle elements");
        Handles();
        Console.WriteLine("PASS: PooledObject returns its element when disposed");
        Collections();
        Console.WriteLine("PASS: ListPool, HashSetPool and DictionaryPool hand out cleared collections");
        Generic();
        Console.WriteLine("PASS: GenericPool and UnsafeGenericPool reuse released objects");
        Linked();
        Console.WriteLine("PASS: LinkedPool counts, reuses and limits elements");
        Vertex();
        Console.WriteLine("PASS: UIVertex.simpleVert is a white vertex facing the camera");
    }

    private sealed class Item { public int Id; }

    private static void CountsAndHooks()
    {
        var log = new List<string>();
        var pool = new ObjectPool<Item>(() => { log.Add("create"); return new Item(); }, _ => log.Add("get"), _ => log.Add("release"), _ => log.Add("destroy"));
        var a = pool.Get();
        var b = pool.Get();
        Expect.That(pool.CountAll == 2 && pool.CountActive == 2 && pool.CountInactive == 0, "two created and active");
        pool.Release(a);
        Expect.That(pool.CountAll == 2 && pool.CountActive == 1 && pool.CountInactive == 1, "one returned");
        Expect.That(ReferenceEquals(pool.Get(), a), "the idle element is reused");
        Expect.Sequence(new[] { "create", "get", "create", "get", "release", "get" }, log, "hook order");
        pool.Release(b);
        Expect.That(pool.CountInactive == 1, "released element is idle");
    }

    private static void DoubleRelease()
    {
        var pool = new ObjectPool<Item>(() => new Item());
        var item = pool.Get();
        pool.Release(item);
        Expect.Throws<InvalidOperationException>(() => pool.Release(item), "double release");
        var loose = new ObjectPool<Item>(() => new Item(), collectionCheck: false);
        var other = loose.Get();
        loose.Release(other);
        loose.Release(other);
        Expect.That(loose.CountInactive == 2, "unchecked pool keeps both");
    }

    private static void MaxSize()
    {
        var destroyed = new List<Item>();
        var pool = new ObjectPool<Item>(() => new Item(), null, null, destroyed.Add, true, 1, 1);
        var a = pool.Get();
        var b = pool.Get();
        pool.Release(a);
        pool.Release(b);
        Expect.That(pool.CountInactive == 1 && destroyed.Count == 1 && ReferenceEquals(destroyed[0], b), "second release destroyed");
        Expect.That(pool.CountAll == 1 && pool.CountActive == 0, "destroyed element no longer counts");
        Expect.Throws<ArgumentException>(() => new ObjectPool<Item>(() => new Item(), maxSize: 0), "zero maxSize");
        Expect.Throws<ArgumentException>(() => new ObjectPool<Item>(() => new Item(), maxSize: -1), "negative maxSize");
    }

    private static void ClearAndDispose()
    {
        var destroyed = 0;
        var pool = new ObjectPool<Item>(() => new Item(), null, null, _ => destroyed++);
        var items = new[] { pool.Get(), pool.Get(), pool.Get() };
        foreach (var item in items) { pool.Release(item); }
        pool.Clear();
        Expect.That(destroyed == 3 && pool.CountInactive == 0 && pool.CountAll == 0, "clear destroys idle elements");
        pool.Release(pool.Get());
        pool.Dispose();
        Expect.That(destroyed == 4 && pool.CountInactive == 0, "dispose clears");
    }

    private static void Handles()
    {
        var pool = new ObjectPool<Item>(() => new Item());
        using (pool.Get(out var item))
        {
            item.Id = 5;
            Expect.That(pool.CountActive == 1, "active inside using");
        }
        Expect.That(pool.CountInactive == 1 && pool.CountActive == 0, "released after using");
        default(PooledObject<Item>).Dispose();
    }

    private static void Collections()
    {
        var list = ListPool<int>.Get();
        list.Add(3);
        ListPool<int>.Release(list);
        var again = ListPool<int>.Get();
        Expect.That(ReferenceEquals(list, again) && again.Count == 0, "list cleared and reused");
        ListPool<int>.Release(again);
        using (HashSetPool<string>.Get(out var set)) { set.Add("a"); }
        Expect.That(HashSetPool<string>.Get().Count == 0, "set cleared");
        var map = DictionaryPool<int, string>.Get();
        map[1] = "x";
        DictionaryPool<int, string>.Release(map);
        Expect.That(DictionaryPool<int, string>.Get().Count == 0, "dictionary cleared");
        Expect.Throws<InvalidOperationException>(() => ListPool<int>.Release(again), "list double release");
    }

    private static void Generic()
    {
        var item = GenericPool<Item>.Get();
        GenericPool<Item>.Release(item);
        Expect.That(ReferenceEquals(item, GenericPool<Item>.Get()), "generic reuse");
        var loose = UnsafeGenericPool<Item>.Get();
        UnsafeGenericPool<Item>.Release(loose);
        UnsafeGenericPool<Item>.Release(loose);
    }

    private static void Linked()
    {
        var pool = new LinkedPool<Item>(() => new Item(), maxSize: 1);
        var a = pool.Get();
        var b = pool.Get();
        pool.Release(a);
        pool.Release(b);
        Expect.That(pool.CountInactive == 1 && pool.CountAll == 1, "linked limit");
        Expect.That(ReferenceEquals(pool.Get(), a), "linked reuse");
        Expect.Throws<ArgumentException>(() => new LinkedPool<Item>(() => new Item(), maxSize: 0), "linked zero maxSize");
    }

    private static void Vertex()
    {
        var v = UIVertex.simpleVert;
        Expect.That(v.position == Vector3.zero && v.normal == new Vector3(0, 0, -1), "position and normal");
        Expect.That(v.tangent == new Vector4(1, 0, 0, -1), "tangent");
        Expect.That(v.color.r == 255 && v.color.g == 255 && v.color.b == 255 && v.color.a == 255, "color");
        Expect.That(v.uv0 == Vector4.zero && v.uv1 == Vector4.zero && v.uv2 == Vector4.zero && v.uv3 == Vector4.zero, "uvs");
    }
}
