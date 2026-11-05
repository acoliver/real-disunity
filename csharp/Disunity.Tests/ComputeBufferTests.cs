using Disunity.Hosting;
using Unity.Collections;
using UnityEngine;
using static ComputeTestSupport;

/// <summary>ComputeBuffer contents: SetData and GetData move bytes through the world's compute buffer, GetData flushes first,
/// and the shape is checked by the world.</summary>
internal static class ComputeBufferTests
{
    private struct Packed
    {
        public uint data;
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            SetDataThenGetDataRoundTrips();
            Console.WriteLine("PASS: ComputeBuffer.SetData followed by GetData round-trips arrays, lists, native arrays and offsets");
            RangesAreChecked();
            Console.WriteLine("PASS: ComputeBuffer SetData and GetData refuse a range outside the array or the buffer, counted in bytes of the array's elements");
            ElementsNeedNotMatchTheStride();
            Console.WriteLine("PASS: ComputeBuffer SetData and GetData move the bytes of elements of any size, as long as the byte range fits the buffer");
            GetDataFlushesAndReadsWhatTheHostWrote();
            Console.WriteLine("PASS: ComputeBuffer.GetData runs the GPU flush before it copies, and a flush that fails fails GetData");
            ReleaseAndShape();
            Console.WriteLine("PASS: a released ComputeBuffer refuses contents calls and keeps its shape, and the world refuses an unusable shape");
        }
        finally
        {
            Unregister(DisunityHost.WorldHandle);
            DisunityHost.UnloadWorld();
        }
    }

    private static void SetDataThenGetDataRoundTrips()
    {
        Reset(Behavior.Nothing);
        using var buffer = new ComputeBuffer(4, 4);
        Expect.Sequence(new[] { 0, 0, 0, 0 }, Get(buffer), "a new buffer is zeroed");
        buffer.SetData(new[] { 1, 2, 3, 4 });
        Expect.Sequence(new[] { 1, 2, 3, 4 }, Get(buffer), "an array round-trips");
        buffer.SetData(new List<int> { 9, 8 });
        Expect.Sequence(new[] { 9, 8, 3, 4 }, Get(buffer), "a list writes from the start");
        buffer.SetData(new[] { 10, 11, 12, 13 }, 1, 2, 2);
        Expect.Sequence(new[] { 9, 8, 11, 12 }, Get(buffer), "an offset array range lands at the buffer offset");
        using (var native = new NativeArray<int>(new[] { 5, 6, 7 }, Allocator.Temp))
        {
            buffer.SetData(native);
            Expect.Sequence(new[] { 5, 6, 7, 12 }, Get(buffer), "a native array writes from the start");
            buffer.SetData(native, 2, 3, 1);
            Expect.Sequence(new[] { 5, 6, 7, 7 }, Get(buffer), "a native array range lands at the buffer offset");
        }
        var part = new int[5];
        buffer.GetData(part, 1, 2, 2);
        Expect.Sequence(new[] { 0, 7, 7, 0, 0 }, part, "GetData with offsets fills the array range only");
        using var packed = new ComputeBuffer(2, 4);
        packed.SetData(new[] { new Packed { data = 0xDEADBEEF }, new Packed { data = 7 } });
        var back = new Packed[2];
        packed.GetData(back);
        Expect.Equal(0xDEADBEEFu, back[0].data, "a struct array round-trips");
        Expect.Equal(7u, back[1].data, "the second struct");
        using var wide = new ComputeBuffer(2, 16);
        wide.SetData(new[] { new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8) });
        var vectors = new Vector4[2];
        wide.GetData(vectors);
        Expect.That(vectors[1] == new Vector4(5, 6, 7, 8), "vectors round-trip at a stride of 16");
    }

    private static int[] Get(ComputeBuffer buffer)
    {
        var data = new int[buffer.count];
        buffer.GetData(data);
        return data;
    }

    private static void RangesAreChecked()
    {
        using var buffer = new ComputeBuffer(4, 4);
        Expect.Throws<ArgumentException>(() => buffer.SetData(new long[3]), "24 bytes in a buffer of 16");
        Expect.Throws<ArgumentException>(() => buffer.GetData(new byte[17]), "17 bytes from a buffer of 16");
        Expect.Throws<ArgumentException>(() => buffer.SetData(new int[5]), "more elements than the buffer holds");
        Expect.Throws<ArgumentException>(() => buffer.SetData(new int[2], 0, 3, 2), "a range that runs past the end of the buffer");
        Expect.Throws<ArgumentException>(() => buffer.SetData(new long[1], 0, 2, 1), "an 8 byte element at byte 16 of a buffer of 16");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(new int[2], 1, 0, 2), "a range that runs past the end of the array");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(new int[2], -1, 0, 1), "a negative array start");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.GetData(new int[2], 0, -1, 1), "a negative buffer start");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(new List<int> { 1 }, 0, 0, 2), "a list range past its end");
        Expect.Throws<ArgumentNullException>(() => buffer.SetData((Array)null!), "a null array");
        Expect.Throws<ArgumentException>(() => buffer.SetData(new[] { "a" }), "an element type without a fixed size");
        Expect.Sequence(new[] { 0, 0, 0, 0 }, Get(buffer), "rejected calls wrote nothing");
    }

    private static unsafe void GetDataFlushesAndReadsWhatTheHostWrote()
    {
        var world = DisunityHost.WorldHandle;
        var shader = Planned(TerrainPlan);
        using var buffer = new ComputeBuffer(4, 4);
        buffer.SetData(new[] { 1, 1, 1, 1 });
        Reset(Behavior.ExecuteDispatches);
        Register(world);
        shader.SetBuffer(0, "_Output", buffer);
        shader.Dispatch(0, 3, 2, 1);
        Expect.Equal(0, Flushes, "the flush runs when a result is read, not when the work is recorded");
        Expect.Sequence(new[] { 6, 1, 1, 1 }, Get(buffer), "GetData sees what the executor wrote for the dispatch (kernel 0 plus 3 x 2 x 1 groups)");
        Expect.Equal(1, Flushes, "one GetData flushes once");
        Expect.Equal(1, Executed.Count, "the executor took the one dispatch");
        Expect.That(BindingNames.Contains("_Output"), "the executor read the buffer binding of the dispatch");
        Get(buffer);
        Expect.Equal(2, Flushes, "each GetData flushes");
        Expect.Equal(1, Executed.Count, "work already taken does not run again");
        Reset(Behavior.Fail);
        var failure = Expect.Catch<DisunityException>(() => Get(buffer), "a failed flush fails GetData");
        Expect.That(failure.Message.Contains("GPU flush") && failure.Message.Contains("the executor has no device"), "the exception names the flush and carries the host's text: " + failure.Message);
        Reset(Behavior.Nothing);
        Unregister(world);
        Expect.Sequence(new[] { 6, 1, 1, 1 }, Get(buffer), "with no executor GetData returns the CPU contents");
        Expect.Equal(0, Flushes, "an unregistered executor is not called");
    }

    private static void ElementsNeedNotMatchTheStride()
    {
        using var args = new ComputeBuffer(1, 20, ComputeBufferType.DrawIndirect);
        args.SetData(new uint[] { 6, 1, 0, 0, 0 });
        var back = new uint[5];
        args.GetData(back);
        Expect.Sequence(new uint[] { 6, 1, 0, 0, 0 }, back, "five 4 byte elements fill a buffer of one 20 byte element");
        using var buffer = new ComputeBuffer(4, 4);
        buffer.SetData(new long[] { 0x0000000200000001L, 0x0000000400000003L });
        Expect.Sequence(new[] { 1, 2, 3, 4 }, Get(buffer), "8 byte elements fill the buffer as their bytes");
        buffer.SetData(new byte[] { 9, 0, 0, 0 }, 0, 4, 4);
        var bytes = new byte[16];
        buffer.GetData(bytes);
        Expect.Equal<byte>(9, bytes[4], "a byte offset counts bytes of the array's own element size");
        var wide = new long[2];
        buffer.GetData(wide, 0, 1, 1);
        Expect.Equal(0x0000000400000003L, wide[0], "an offset in 8 byte elements lands on byte 8");
    }

    private static void ReleaseAndShape()
    {
        var buffer = new ComputeBuffer(4, 4);
        buffer.SetData(new[] { 1, 2, 3, 4 });
        Expect.That(buffer.IsValid(), "a new buffer is valid");
        buffer.Release();
        buffer.Release();
        buffer.Dispose();
        Expect.That(!buffer.IsValid(), "a released buffer is not valid");
        Expect.That(buffer.count == 4 && buffer.stride == 4, "the shape survives Release");
        Expect.Throws<ObjectDisposedException>(() => buffer.SetData(new int[4]), "SetData after Release");
        Expect.Throws<ObjectDisposedException>(() => buffer.GetData(new int[4]), "GetData after Release");
        var odd = new ComputeBuffer(4, 6);
        Expect.Throws<ArgumentException>(() => odd.SetData(new byte[0]), "a stride that is not a multiple of 4");
        var huge = new ComputeBuffer(int.MaxValue, 1024);
        Expect.Throws<ArgumentException>(() => huge.GetData(new int[0]), "a buffer over the size limit");
    }
}
