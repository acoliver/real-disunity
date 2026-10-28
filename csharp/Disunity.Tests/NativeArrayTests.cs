using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;

/// <summary>
/// <c>NativeArray&lt;T&gt;</c> as a game uses it: allocation, element access, copying, views,
/// disposal and the exceptions Unity's player throws for misuse.
/// </summary>
internal static unsafe class NativeArrayTests
{
    public static void Run()
    {
        AllocatesZeroedOrUninitializedMemory();
        ReadsAndWritesElementsWithinBounds();
        BuildsFromManagedAndNativeSources();
        CopiesWholeArraysOnlyWhenLengthsMatch();
        CopiesRangesAndRejectsOutOfRangeOnes();
        SubArraysShareMemory();
        DisposeFreesOnceAndLeavesTheArrayEmpty();
        RejectsInvalidAllocationArguments();
        ReadOnlyViewsSeeTheArray();
        EnumeratesInOrder();
        ComparesByBufferAndLength();
        ReinterpretsAsAnotherElementType();
        Console.WriteLine("PASS: NativeArray allocation, access, copies, views, disposal and misuse errors");
    }

    private static void AllocatesZeroedOrUninitializedMemory()
    {
        using (var dirty = new NativeArray<int>(256, Allocator.Persistent, NativeArrayOptions.UninitializedMemory))
            UnsafeUtility.MemSet(dirty.GetUnsafePtr(), 0xAB, 256 * sizeof(int));
        var cleared = new NativeArray<int>(256, Allocator.Persistent, NativeArrayOptions.ClearMemory);
        Expect.Equal(256, cleared.Length, "length");
        Expect.That(cleared.IsCreated, "a new array is created");
        Expect.That(cleared.ToArray().All(value => value == 0), "ClearMemory zeroes every element, including reused memory");
        var defaulted = new NativeArray<long>(8, Allocator.TempJob);
        Expect.That(defaulted.ToArray().All(value => value == 0), "memory is cleared unless uninitialized memory is asked for");
        var uninitialized = new NativeArray<byte>(16, Allocator.Temp, NativeArrayOptions.UninitializedMemory);
        uninitialized[15] = 7;
        Expect.Equal(16, uninitialized.Length, "an uninitialized array has the requested length");
        Expect.Equal((byte)7, uninitialized[15], "an uninitialized array is writable");
        var empty = new NativeArray<int>(0, Allocator.Persistent);
        Expect.That(empty.IsCreated && empty.Length == 0, "a zero-length array is created and empty");
        cleared.Dispose();
        defaulted.Dispose();
        uninitialized.Dispose();
        empty.Dispose();
    }

    private static void ReadsAndWritesElementsWithinBounds()
    {
        var array = new NativeArray<int>(4, Allocator.Persistent);
        for (var i = 0; i < array.Length; i++) array[i] = i * 10 + 1;
        Expect.Sequence(new[] { 1, 11, 21, 31 }, array.ToArray(), "written elements");
        Expect.Equal(21, array[2], "indexer read");
        Expect.Throws<IndexOutOfRangeException>(() => _ = array[4], "read at Length");
        Expect.Throws<IndexOutOfRangeException>(() => _ = array[-1], "read at -1");
        Expect.Throws<IndexOutOfRangeException>(() => array[4] = 1, "write at Length");
        var structs = new NativeArray<Pair>(2, Allocator.Persistent);
        structs[1] = new Pair { Id = 5, Weight = 2.5f };
        Expect.That(structs[1].Id == 5 && structs[1].Weight == 2.5f && structs[0].Id == 0, "struct elements keep their fields");
        array.Dispose();
        structs.Dispose();
    }

    private static void BuildsFromManagedAndNativeSources()
    {
        int[] source = [3, 1, 4, 1, 5];
        var fromArray = new NativeArray<int>(source, Allocator.Persistent);
        source[0] = 99;
        Expect.Sequence(new[] { 3, 1, 4, 1, 5 }, fromArray.ToArray(), "construction copies the managed array");
        var fromNative = new NativeArray<int>(fromArray, Allocator.Persistent);
        fromArray[1] = 42;
        Expect.Equal(1, fromNative[1], "construction from a native array copies it");
        var fromView = new NativeArray<int>(fromArray.AsReadOnly(), Allocator.Persistent);
        Expect.Sequence(new[] { 3, 42, 4, 1, 5 }, fromView.ToArray(), "construction from a read-only view");
        Expect.Throws<ArgumentNullException>(() => new NativeArray<int>((int[])null!, Allocator.Persistent), "null source array");
        fromArray.Dispose();
        fromNative.Dispose();
        fromView.Dispose();
    }

    private static void CopiesWholeArraysOnlyWhenLengthsMatch()
    {
        var array = new NativeArray<int>(3, Allocator.Persistent);
        array.CopyFrom([7, 8, 9]);
        Expect.Sequence(new[] { 7, 8, 9 }, array.ToArray(), "CopyFrom(T[])");
        var managed = new int[3];
        array.CopyTo(managed);
        Expect.Sequence(new[] { 7, 8, 9 }, managed, "CopyTo(T[])");
        var other = new NativeArray<int>(3, Allocator.Persistent);
        other.CopyFrom(array);
        array[0] = 0;
        Expect.Sequence(new[] { 7, 8, 9 }, other.ToArray(), "CopyFrom(NativeArray)");
        array.CopyTo(other);
        Expect.Sequence(new[] { 0, 8, 9 }, other.ToArray(), "CopyTo(NativeArray)");
        NativeArray<int>.Copy([1, 2, 3], array);
        Expect.Sequence(new[] { 1, 2, 3 }, array.ToArray(), "Copy(T[], NativeArray)");
        NativeArray<int>.Copy(array, managed);
        NativeArray<int>.Copy(other, array);
        Expect.Sequence(new[] { 1, 2, 3 }, managed, "Copy(NativeArray, T[])");
        Expect.Sequence(new[] { 0, 8, 9 }, array.ToArray(), "Copy(NativeArray, NativeArray)");
        Expect.Throws<ArgumentException>(() => array.CopyFrom(new int[2]), "CopyFrom with a shorter array");
        Expect.Throws<ArgumentException>(() => array.CopyFrom(new int[4]), "CopyFrom with a longer array");
        Expect.Throws<ArgumentException>(() => array.CopyTo(new int[4]), "CopyTo a longer array");
        Expect.Throws<ArgumentException>(() => NativeArray<int>.Copy(array, new int[2]), "Copy to a shorter array");
        Expect.Throws<ArgumentException>(() => NativeArray<int>.Copy(new int[5], array), "Copy from a longer array");
        array.Dispose();
        other.Dispose();
    }

    private static void CopiesRangesAndRejectsOutOfRangeOnes()
    {
        var native = new NativeArray<int>([10, 11, 12, 13, 14, 15], Allocator.Persistent);
        var target = new NativeArray<int>(6, Allocator.Persistent);
        NativeArray<int>.Copy(native, 2, target, 1, 3);
        Expect.Sequence(new[] { 0, 12, 13, 14, 0, 0 }, target.ToArray(), "native to native range");
        var managed = new int[6];
        NativeArray<int>.Copy(native, 4, managed, 0, 2);
        Expect.Sequence(new[] { 14, 15, 0, 0, 0, 0 }, managed, "native to managed range");
        NativeArray<int>.Copy([1, 2, 3], 1, target, 4, 2);
        Expect.Sequence(new[] { 0, 12, 13, 14, 2, 3 }, target.ToArray(), "managed to native range");
        NativeArray<int>.Copy(native.AsReadOnly(), 0, target, 0, 2);
        Expect.Sequence(new[] { 10, 11, 13, 14, 2, 3 }, target.ToArray(), "read-only view to native range");
        NativeArray<int>.Copy(new[] { 9, 9 }, target, 2);
        Expect.Sequence(new[] { 9, 9, 13, 14, 2, 3 }, target.ToArray(), "managed copy of a leading length");
        NativeArray<int>.Copy(native, target, 4);
        Expect.Sequence(new[] { 10, 11, 12, 13, 2, 3 }, target.ToArray(), "native copy of a leading length");
        NativeArray<int>.Copy(native, managed, 3);
        Expect.Sequence(new[] { 10, 11, 12, 0, 0, 0 }, managed, "native to managed leading length");
        NativeArray<int>.Copy(native, 3, target, 0, 0);
        Expect.Sequence(new[] { 10, 11, 12, 13, 2, 3 }, target.ToArray(), "a zero-length copy changes nothing");
        Expect.Throws<ArgumentOutOfRangeException>(() => NativeArray<int>.Copy(native, 4, target, 0, 3), "source range past the end");
        Expect.Throws<ArgumentOutOfRangeException>(() => NativeArray<int>.Copy(native, 0, target, 4, 3), "destination range past the end");
        Expect.Throws<ArgumentOutOfRangeException>(() => NativeArray<int>.Copy(native, -1, target, 0, 1), "negative source index");
        Expect.Throws<ArgumentOutOfRangeException>(() => NativeArray<int>.Copy(native, 0, target, 0, -1), "negative length");
        Expect.Throws<ArgumentOutOfRangeException>(() => NativeArray<int>.Copy([1], 0, target, 0, 2), "managed source range past the end");
        native.Dispose();
        target.Dispose();
    }

    private static void SubArraysShareMemory()
    {
        var array = new NativeArray<int>([0, 1, 2, 3, 4, 5], Allocator.Persistent);
        var sub = array.GetSubArray(2, 3);
        Expect.Equal(3, sub.Length, "sub-array length");
        Expect.Sequence(new[] { 2, 3, 4 }, sub.ToArray(), "sub-array elements");
        sub[0] = 20;
        array[4] = 40;
        Expect.Equal(20, array[2], "a sub-array write reaches the array");
        Expect.Equal(40, sub[2], "an array write reaches the sub-array");
        Expect.Throws<IndexOutOfRangeException>(() => _ = sub[3], "sub-array bounds");
        Expect.Equal(0, array.GetSubArray(6, 0).Length, "an empty sub-array at the end");
        Expect.Equal(6, array.GetSubArray(0, 6).Length, "a sub-array of the whole array");
        Expect.Throws<ArgumentOutOfRangeException>(() => array.GetSubArray(4, 3), "sub-array past the end");
        Expect.Throws<ArgumentOutOfRangeException>(() => array.GetSubArray(-1, 2), "sub-array before the start");
        Expect.Throws<ArgumentOutOfRangeException>(() => array.GetSubArray(0, -1), "sub-array of negative length");
        Expect.Throws<InvalidOperationException>(sub.Dispose, "a sub-array has Allocator.None and is not disposable");
        Expect.That(array.IsCreated && array[0] == 0, "the failed dispose of a sub-array leaves the array alone");
        array.Dispose();
    }

    /// <summary>Unity's player has no safety handle to name a double dispose: Dispose clears the
    /// allocator label, and Dispose throws InvalidOperationException for a label that is not a
    /// real allocator. Reading a disposed array is out of range because its length is zero.</summary>
    private static void DisposeFreesOnceAndLeavesTheArrayEmpty()
    {
        var array = new NativeArray<int>([1, 2, 3], Allocator.Persistent);
        Expect.That(array.IsCreated, "created before Dispose");
        array.Dispose();
        Expect.That(!array.IsCreated && array.Length == 0, "an array is empty after Dispose");
        Expect.Throws<IndexOutOfRangeException>(() => _ = array[0], "use after Dispose");
        Expect.Throws<InvalidOperationException>(array.Dispose, "second Dispose");
        Expect.That(!default(NativeArray<int>).IsCreated, "a default array is not created");
        Expect.Throws<InvalidOperationException>(default(NativeArray<int>).Dispose, "Dispose of a default array");
        var later = new NativeArray<int>(2, Allocator.TempJob);
        var handle = later.Dispose(default);
        Expect.That(!later.IsCreated && handle.IsCompleted, "Dispose(JobHandle) frees the array and returns a complete handle");
        var over = new NativeArray<int>(2, Allocator.Persistent);
        var wrapper = NativeArrayUnsafeUtility.ConvertExistingDataToNativeArray<int>(over.GetUnsafePtr(), 2, Allocator.Persistent);
        wrapper.Dispose();
        Expect.That(!wrapper.IsCreated, "an array wrapped over memory with a real allocator frees it");
    }

    private static void RejectsInvalidAllocationArguments()
    {
        Expect.Throws<ArgumentException>(() => new NativeArray<int>(4, Allocator.None), "Allocator.None");
        Expect.Throws<ArgumentException>(() => new NativeArray<int>(4, Allocator.Invalid), "Allocator.Invalid");
        Expect.Throws<ArgumentException>(() => new NativeArray<int>(new int[2], Allocator.None), "Allocator.None from a managed array");
        Expect.Throws<ArgumentOutOfRangeException>(() => new NativeArray<int>(-1, Allocator.Persistent), "negative length");
    }

    private static void ReadOnlyViewsSeeTheArray()
    {
        var array = new NativeArray<int>([5, 6, 7], Allocator.Persistent);
        var view = array.AsReadOnly();
        Expect.That(view.IsCreated && view.Length == 3, "view length");
        array[1] = 60;
        Expect.Equal(60, view[1], "the view sees later writes");
        Expect.Sequence(new[] { 5, 60, 7 }, view.ToArray(), "view ToArray");
        Expect.Throws<IndexOutOfRangeException>(() => _ = view[3], "view bounds");
        var managed = new int[3];
        view.CopyTo(managed);
        Expect.Sequence(new[] { 5, 60, 7 }, managed, "view CopyTo(T[])");
        Expect.Sequence(new[] { 5, 60, 7 }, view.Select(value => value), "view enumeration");
        Expect.Equal(7, view.AsReadOnlySpan()[2], "view span");
        Expect.That(typeof(NativeArray<int>.ReadOnly).GetProperty("Item")!.SetMethod is null, "a view has no indexer setter");
        array.Dispose();
    }

    private static void EnumeratesInOrder()
    {
        var array = new NativeArray<int>([4, 5, 6], Allocator.Persistent);
        var seen = new List<int>();
        foreach (var value in array) seen.Add(value);
        Expect.Sequence(new[] { 4, 5, 6 }, seen, "foreach");
        Expect.Equal(15, array.Sum(), "LINQ over IEnumerable<T>");
        var enumerator = array.GetEnumerator();
        Expect.That(enumerator.MoveNext() && enumerator.Current == 4, "first element");
        enumerator.Reset();
        Expect.That(enumerator.MoveNext() && enumerator.Current == 4, "Reset starts again");
        var empty = new NativeArray<int>(0, Allocator.Persistent);
        Expect.That(!empty.GetEnumerator().MoveNext(), "an empty array has no elements");
        array.Dispose();
        empty.Dispose();
    }

    private static void ComparesByBufferAndLength()
    {
        var array = new NativeArray<int>([1, 2, 3], Allocator.Persistent);
        var same = new NativeArray<int>([1, 2, 3], Allocator.Persistent);
        var copy = array;
        Expect.That(copy == array && copy.Equals(array) && copy.Equals((object)array), "a copy of the struct is equal");
        Expect.Equal(array.GetHashCode(), copy.GetHashCode(), "equal arrays hash alike");
        Expect.That(array != same && !array.Equals(same), "arrays over different memory are not equal even with equal elements");
        Expect.That(array.GetSubArray(0, 3) == array, "a sub-array over the whole array equals it");
        Expect.That(array.GetSubArray(0, 2) != array, "a shorter sub-array differs");
        Expect.That(!array.Equals("array"), "an array is not equal to another type");
        Expect.That(default(NativeArray<int>) == default, "default arrays are equal");
        array.Dispose();
        same.Dispose();
    }

    private static void ReinterpretsAsAnotherElementType()
    {
        var ints = new NativeArray<int>([0x04030201, 0x08070605], Allocator.Persistent);
        var bytes = ints.Reinterpret<byte>();
        Expect.Equal(8, bytes.Length, "int to byte length");
        Expect.Sequence(new byte[] { 1, 2, 3, 4, 5, 6, 7, 8 }, bytes.ToArray(), "int to byte elements");
        bytes[0] = 0xFF;
        Expect.Equal(0x040302FF, ints[0], "a write through the reinterpreted array reaches the original");
        var shorts = ints.Reinterpret<short>(sizeof(int));
        Expect.Equal(4, shorts.Length, "int to short length with the expected size");
        Expect.Throws<InvalidOperationException>(() => ints.Reinterpret<short>(2), "a wrong expected source size");
        var three = new NativeArray<byte>(3, Allocator.Persistent);
        Expect.Throws<InvalidOperationException>(() => three.Reinterpret<int>(), "bytes that do not divide into whole elements");
        Expect.Equal(0x08070605, ints.ReinterpretLoad<int>(1), "ReinterpretLoad");
        ints.ReinterpretStore(1, 0x11223344);
        Expect.Equal(0x11223344, ints[1], "ReinterpretStore");
        Expect.Equal(4, ints.AsReadOnly().Reinterpret<short>().Length, "a read-only view reinterprets by size");
        ints.Dispose();
        three.Dispose();
    }

    private struct Pair
    {
        public int Id;
        public float Weight;
    }
}
