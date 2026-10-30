using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Jobs;

/// <summary>TransformAccessArray as a managed-side list of transforms.</summary>
internal static class TransformAccessArrayTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Listing();
            Removing();
            Lifetime();
            Arguments();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: TransformAccessArray adds, removes, indexes and disposes");
    }

    private static Transform[] Make(int count)
    {
        var transforms = new Transform[count];
        for (var i = 0; i < count; i++) transforms[i] = DisunityHost.CreateGameObject().transform;
        return transforms;
    }

    private static void Listing()
    {
        var t = Make(3);
        var array = new TransformAccessArray(2);
        Expect.That(array.isCreated && array.length == 0 && array.capacity == 2, "a new array is empty with the capacity asked");
        array.Add(t[0]);
        array.Add(t[1]);
        Expect.Equal(2, array.capacity, "capacity holds while the length fits");
        array.Add(t[2]);
        Expect.That(array.length == 3 && array.capacity >= 3, "capacity grows past the length");
        Expect.That(ReferenceEquals(array[0], t[0]) && ReferenceEquals(array[2], t[2]), "indexer reads");
        array[0] = t[2];
        Expect.That(ReferenceEquals(array[0], t[2]), "indexer writes");
        array.capacity = 16;
        Expect.Equal(16, array.capacity, "capacity is settable");
        var copy = array;
        copy.Add(t[0]);
        Expect.Equal(4, array.length, "copies share one list");
        array.Dispose();

        var fromArray = new TransformAccessArray(t);
        Expect.That(fromArray.length == 3 && ReferenceEquals(fromArray[1], t[1]), "built from an array");
        fromArray.SetTransforms(new[] { t[2] });
        Expect.That(fromArray.length == 1 && ReferenceEquals(fromArray[0], t[2]), "SetTransforms replaces the contents");
        var small = new TransformAccessArray(1);
        small.SetTransforms(t);
        Expect.That(small.length == 3 && small.capacity >= 3, "SetTransforms grows the capacity to fit");
        small.Dispose();
        fromArray.Dispose();
        TransformAccessArray.Allocate(5, 1, out var allocated);
        Expect.That(allocated.isCreated && allocated.capacity == 5, "Allocate makes an array");
        allocated.Dispose();
    }

    private static void Removing()
    {
        var t = Make(4);
        var array = new TransformAccessArray(t);
        array.RemoveAtSwapBack(1);
        Expect.That(array.length == 3 && ReferenceEquals(array[1], t[3]) && ReferenceEquals(array[0], t[0]) && ReferenceEquals(array[2], t[2]), "the last transform fills the gap");
        array.RemoveAtSwapBack(2);
        Expect.That(array.length == 2 && ReferenceEquals(array[1], t[3]), "removing the last drops it");
        Expect.Throws<IndexOutOfRangeException>(() => array.RemoveAtSwapBack(2), "remove past the end");
        Expect.Throws<IndexOutOfRangeException>(() => array.RemoveAtSwapBack(-1), "remove before the start");
        Expect.Throws<IndexOutOfRangeException>(() => _ = array[2], "read past the end");
        Expect.Throws<IndexOutOfRangeException>(() => array[2] = t[0], "write past the end");
        array.RemoveAtSwapBack(1);
        array.RemoveAtSwapBack(0);
        Expect.Equal(0, array.length, "emptied");
        Expect.Throws<IndexOutOfRangeException>(() => array.RemoveAtSwapBack(0), "remove from an empty array");
        array.Dispose();
    }

    private static void Lifetime()
    {
        var t = Make(1);
        var array = new TransformAccessArray(t);
        var copy = array;
        array.Dispose();
        Expect.That(!array.isCreated && !copy.isCreated, "disposing disposes every copy");
        Expect.Throws<InvalidOperationException>(() => array.Add(t[0]), "Add after Dispose");
        Expect.Throws<InvalidOperationException>(() => _ = array.length, "length after Dispose");
        Expect.Throws<InvalidOperationException>(() => _ = array.capacity, "capacity after Dispose");
        Expect.Throws<InvalidOperationException>(() => _ = array[0], "indexer after Dispose");
        Expect.Throws<InvalidOperationException>(() => array.RemoveAtSwapBack(0), "RemoveAtSwapBack after Dispose");
        Expect.Throws<InvalidOperationException>(() => array.SetTransforms(t), "SetTransforms after Dispose");
        Expect.Throws<InvalidOperationException>(() => array.Dispose(), "a second Dispose");
        var none = default(TransformAccessArray);
        Expect.That(!none.isCreated, "a default array is not created");
        Expect.Throws<InvalidOperationException>(() => none.Add(t[0]), "Add to a default array");
    }

    private static void Arguments()
    {
        var t = Make(2);
        var array = new TransformAccessArray(t);
        Expect.Throws<ArgumentNullException>(() => array.Add(null!), "Add(null)");
        Expect.Throws<ArgumentNullException>(() => array[0] = null!, "indexer set null");
        Expect.Throws<ArgumentNullException>(() => array.SetTransforms(null!), "SetTransforms(null)");
        Expect.Throws<ArgumentNullException>(() => array.SetTransforms(new Transform[] { t[0], null! }), "SetTransforms with a null entry");
        Expect.Equal(2, array.length, "a rejected SetTransforms leaves the contents");
        Expect.Throws<ArgumentOutOfRangeException>(() => array.capacity = 1, "capacity below the length");
        Expect.Throws<ArgumentOutOfRangeException>(() => new TransformAccessArray(-1), "negative capacity");
        Expect.Throws<ArgumentNullException>(() => new TransformAccessArray((Transform[])null!), "null array");
        array.Dispose();
    }
}
