using System.Runtime.InteropServices;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;

/// <summary><c>NativeSlice&lt;T&gt;</c> and the <c>UnsafeUtility</c> memory functions beneath the containers.</summary>
internal static unsafe class NativeMemoryTests
{
    public static void Run()
    {
        SlicesAnArrayAndWritesThrough();
        SlicesOneFieldOfAStructArrayByStride();
        CopiesSlicesToAndFromManagedArrays();
        MallocHonoursAlignmentAndRejectsBadArguments();
        CopiesFillsAndComparesBytes();
        ReportsSizeAndAlignment();
        ReadsAndWritesArrayElements();
        Console.WriteLine("PASS: NativeSlice views and UnsafeUtility allocation, byte operations, layout and element access");
    }

    private static void SlicesAnArrayAndWritesThrough()
    {
        var array = new NativeArray<int>([0, 1, 2, 3, 4, 5], Allocator.Persistent);
        var slice = new NativeSlice<int>(array, 2, 3);
        Expect.That(slice.Length == 3 && slice.Stride == sizeof(int), "slice length and stride");
        Expect.Sequence(new[] { 2, 3, 4 }, slice.ToArray(), "slice elements");
        slice[1] = 30;
        Expect.Equal(30, array[3], "a slice write reaches the array");
        array[4] = 40;
        Expect.Equal(40, slice[2], "an array write reaches the slice");
        Expect.Throws<IndexOutOfRangeException>(() => _ = slice[3], "read past the slice");
        Expect.Throws<IndexOutOfRangeException>(() => slice[-1] = 1, "write before the slice");
        Expect.Equal(4, new NativeSlice<int>(array, 2).Length, "a slice from a start runs to the end");
        Expect.Equal(6, new NativeSlice<int>(array).Length, "a slice of the whole array");
        NativeSlice<int> converted = array;
        Expect.That(converted.Length == 6 && converted.Equals(new NativeSlice<int>(array)), "an array converts to a slice of all of it");
        Expect.That(slice.Equals(new NativeSlice<int>(array, 2, 3)) && !slice.Equals(new NativeSlice<int>(array, 2, 2)), "slice equality");
        Expect.Sequence(new[] { 2, 30, 40 }, slice, "enumeration");
        Expect.Throws<ArgumentOutOfRangeException>(() => new NativeSlice<int>(array, 4, 3), "slice past the end");
        Expect.Throws<ArgumentOutOfRangeException>(() => new NativeSlice<int>(array, -1, 2), "slice before the start");
        array.Dispose();
    }

    private static void SlicesOneFieldOfAStructArrayByStride()
    {
        var array = new NativeArray<Vertex>(3, Allocator.Persistent);
        for (var i = 0; i < 3; i++) array[i] = new Vertex { X = i, Id = 100 + i };
        var ids = NativeSliceUnsafeUtility.ConvertExistingDataToNativeSlice<int>((byte*)array.GetUnsafePtr() + sizeof(float), sizeof(Vertex), 3);
        Expect.Equal(sizeof(Vertex), ids.Stride, "stride");
        Expect.Sequence(new[] { 100, 101, 102 }, ids.ToArray(), "one field of each element");
        ids[1] = 7;
        Expect.That(array[1].Id == 7 && array[1].X == 1 && array[0].Id == 100, "a strided write changes only that field");
        Expect.That(ids.GetUnsafePtr() == (byte*)array.GetUnsafePtr() + sizeof(float), "the slice starts at the field");
        array.Dispose();
    }

    private static void CopiesSlicesToAndFromManagedArrays()
    {
        var array = new NativeArray<int>(5, Allocator.Persistent);
        var slice = new NativeSlice<int>(array, 1, 3);
        slice.CopyFrom([7, 8, 9]);
        Expect.Sequence(new[] { 0, 7, 8, 9, 0 }, array.ToArray(), "CopyFrom writes the slice's range only");
        var managed = new int[3];
        slice.CopyTo(managed);
        Expect.Sequence(new[] { 7, 8, 9 }, managed, "CopyTo");
        Expect.Throws<ArgumentException>(() => slice.CopyFrom(new int[2]), "CopyFrom with a shorter array");
        Expect.Throws<ArgumentException>(() => slice.CopyTo(new int[4]), "CopyTo a longer array");
        array.Dispose();
    }

    private static void MallocHonoursAlignmentAndRejectsBadArguments()
    {
        foreach (var alignment in new[] { 1, 4, 16, 64, 256 })
        {
            var pointers = new List<IntPtr>();
            for (var size = 1; size <= 200; size += 13)
            {
                var memory = UnsafeUtility.Malloc(size, alignment, Allocator.Persistent);
                pointers.Add((IntPtr)memory);
                Expect.That((long)memory % alignment == 0, $"a {size} byte block is aligned to {alignment}");
                UnsafeUtility.MemSet(memory, 0x5A, size);
            }
            foreach (var pointer in pointers) UnsafeUtility.Free((void*)pointer, Allocator.Persistent);
        }
        var zero = UnsafeUtility.Malloc(0, 16, Allocator.Temp);
        Expect.That(zero != null, "a zero-byte block is a valid pointer");
        UnsafeUtility.Free(zero, Allocator.Temp);
        Expect.Throws<ArgumentException>(() => UnsafeUtility.Malloc(8, 16, Allocator.None), "Allocator.None");
        Expect.Throws<ArgumentException>(() => UnsafeUtility.Malloc(8, 16, Allocator.Invalid), "Allocator.Invalid");
        Expect.Throws<ArgumentException>(() => UnsafeUtility.Malloc(8, 12, Allocator.Persistent), "an alignment that is not a power of two");
        Expect.Throws<ArgumentOutOfRangeException>(() => UnsafeUtility.Malloc(-1, 4, Allocator.Persistent), "negative size");
        Expect.That(UnsafeUtility.IsValidAllocator(Allocator.Temp) && !UnsafeUtility.IsValidAllocator(Allocator.None), "valid allocators");
    }

    private static void CopiesFillsAndComparesBytes()
    {
        var a = (byte*)UnsafeUtility.Malloc(16, 16, Allocator.Persistent);
        var b = (byte*)UnsafeUtility.Malloc(16, 16, Allocator.Persistent);
        UnsafeUtility.MemSet(a, 0x11, 16);
        UnsafeUtility.MemClear(b, 16);
        Expect.That(new ReadOnlySpan<byte>(a, 16).ToArray().All(v => v == 0x11) && new ReadOnlySpan<byte>(b, 16).ToArray().All(v => v == 0),
            "MemSet and MemClear");
        UnsafeUtility.MemCpy(b + 4, a, 8);
        Expect.Sequence(new byte[] { 0, 0, 0, 0, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0, 0, 0, 0 }, new ReadOnlySpan<byte>(b, 16).ToArray(),
            "MemCpy copies exactly the requested bytes");
        Expect.Equal(0, UnsafeUtility.MemCmp(a, a, 16), "equal memory compares equal");
        Expect.That(UnsafeUtility.MemCmp(a, b, 16) > 0 && UnsafeUtility.MemCmp(b, a, 16) < 0, "MemCmp orders by the first differing byte");
        Expect.Equal(0, UnsafeUtility.MemCmp(a, b + 4, 8), "MemCmp compares only the requested size");
        for (var i = 0; i < 16; i++) a[i] = (byte)i;
        UnsafeUtility.MemMove(a + 2, a, 8);
        Expect.Sequence(new byte[] { 0, 1, 0, 1, 2, 3, 4, 5, 6, 7, 10, 11 }, new ReadOnlySpan<byte>(a, 12).ToArray(), "MemMove handles overlap");
        UnsafeUtility.MemSet(a, 1, 4);
        UnsafeUtility.MemSet(b, 2, 4);
        UnsafeUtility.MemSwap(a, b, 4);
        Expect.That(a[0] == 2 && a[3] == 2 && b[0] == 1 && b[3] == 1 && a[4] == 2, "MemSwap exchanges the bytes");
        UnsafeUtility.MemClear(b, 16);
        UnsafeUtility.MemCpyReplicate(b, a, 2, 4);
        Expect.Sequence(new byte[] { 2, 2, 2, 2, 2, 2, 2, 2, 0 }, new ReadOnlySpan<byte>(b, 9).ToArray(), "MemCpyReplicate");
        UnsafeUtility.MemClear(b, 16);
        UnsafeUtility.MemCpyStride(b, 4, a, 2, 1, 4);
        Expect.Sequence(new byte[] { 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 4, 0, 0, 0 }, new ReadOnlySpan<byte>(b, 16).ToArray(), "MemCpyStride");
        UnsafeUtility.Free(a, Allocator.Persistent);
        UnsafeUtility.Free(b, Allocator.Persistent);
    }

    private static void ReportsSizeAndAlignment()
    {
        Expect.That(UnsafeUtility.SizeOf<byte>() == 1 && UnsafeUtility.SizeOf<short>() == 2 && UnsafeUtility.SizeOf<int>() == 4
            && UnsafeUtility.SizeOf<long>() == 8 && UnsafeUtility.SizeOf<double>() == 8, "primitive sizes");
        Expect.Equal(sizeof(Vertex), UnsafeUtility.SizeOf<Vertex>(), "struct size");
        Expect.Equal(12, UnsafeUtility.SizeOf<UnityEngine.Vector3>(), "Vector3 size");
        Expect.Equal(8, UnsafeUtility.SizeOf(typeof(long)), "size by type");
        Expect.Equal(8, UnsafeUtility.SizeOf(typeof(Vertex)), "struct size by type");
        Expect.That(UnsafeUtility.AlignOf<byte>() == 1 && UnsafeUtility.AlignOf<short>() == 2 && UnsafeUtility.AlignOf<int>() == 4
            && UnsafeUtility.AlignOf<long>() == 8 && UnsafeUtility.AlignOf<double>() == 8, "primitive alignments");
        Expect.Equal(4, UnsafeUtility.AlignOf<Vertex>(), "a struct aligns to its widest field");
        Expect.Equal(8, UnsafeUtility.AlignOf<WideVertex>(), "a struct with a long aligns to 8");
        Expect.Equal(1, UnsafeUtility.AlignOf<Bytes>(), "a struct of bytes aligns to 1");
        var wide = new WideVertex { Id = 1, Flag = 2 };
        var bytes = new Bytes { A = 1, B = 2 };
        Expect.Equal(16, UnsafeUtility.SizeOf<WideVertex>(), "a struct with a long is padded to a multiple of 8");
        Expect.Equal(2, UnsafeUtility.SizeOf<Bytes>(), "a struct of two bytes is two bytes");
        Expect.Equal((byte)2, UnsafeUtility.ReadArrayElement<byte>(UnsafeUtility.AddressOf(ref wide), 8), "the byte follows the long");
        Expect.Equal((byte)2, UnsafeUtility.ReadArrayElement<byte>(UnsafeUtility.AddressOf(ref bytes), 1), "the second byte follows the first");
        Expect.Equal(4, UnsafeUtility.AlignOf<UnityEngine.Vector3>(), "Vector3 aligns to its floats");
        Expect.That(UnsafeUtility.IsUnmanaged<Vertex>() && !UnsafeUtility.IsUnmanaged<string>() && UnsafeUtility.IsUnmanaged(typeof(int)), "unmanaged types");
        Expect.That(UnsafeUtility.IsBlittable(typeof(Vertex)) && !UnsafeUtility.IsBlittable(typeof(string)), "blittable types");
    }

    private static void ReadsAndWritesArrayElements()
    {
        var memory = UnsafeUtility.Malloc(sizeof(Vertex) * 8, UnsafeUtility.AlignOf<Vertex>(), Allocator.Persistent);
        UnsafeUtility.MemClear(memory, sizeof(Vertex) * 8);
        UnsafeUtility.WriteArrayElement(memory, 2, new Vertex { X = 1.5f, Id = 9 });
        var read = UnsafeUtility.ReadArrayElement<Vertex>(memory, 2);
        Expect.That(read.X == 1.5f && read.Id == 9, "a written element reads back");
        Expect.Equal(0, UnsafeUtility.ReadArrayElement<Vertex>(memory, 1).Id, "neighbours are untouched");
        Expect.Equal(9, *(int*)((byte*)memory + 2 * sizeof(Vertex) + sizeof(float)), "the element sits at index times size");
        UnsafeUtility.WriteArrayElementWithStride(memory, 3, 16, 77);
        Expect.Equal(77, *(int*)((byte*)memory + 48), "a strided write lands at index times stride");
        Expect.Equal(77, UnsafeUtility.ReadArrayElementWithStride<int>(memory, 3, 16), "a strided read");
        UnsafeUtility.ArrayElementAsRef<Vertex>(memory, 0).Id = 5;
        Expect.Equal(5, UnsafeUtility.ReadArrayElement<Vertex>(memory, 0).Id, "a reference to an element writes through");
        var copy = new Vertex { X = 2, Id = 3 };
        UnsafeUtility.CopyStructureToPtr(ref copy, memory);
        UnsafeUtility.CopyPtrToStructure<Vertex>(memory, out var back);
        Expect.That(back.X == 2 && back.Id == 3, "a structure copies to a pointer and back");
        UnsafeUtility.Free(memory, Allocator.Persistent);
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Vertex
    {
        public float X;
        public int Id;
    }

    private struct WideVertex
    {
        public long Id;
        public byte Flag;
    }

    private struct Bytes
    {
        public byte A;
        public byte B;
    }
}
