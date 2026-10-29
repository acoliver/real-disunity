using System.Runtime.InteropServices;
using UnityEngine;

/// <summary>GraphicsBuffer: managed-memory contents, validation, handles and release.</summary>
internal static class GraphicsBufferTests
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Wide
    {
        public int A, B, C, D;
    }

    private struct Holder
    {
        public string Name { get; set; }
    }

    private const GraphicsBuffer.Target Raw = GraphicsBuffer.Target.Raw;

    public static void Run()
    {
        ConstructorValidatesItsArguments();
        ReportsCountStrideAndZeroedContents();
        HandlesAreUniqueAndNonZero();
        SetDataCopiesBytesAtElementOffsets();
        SetDataRejectsElementsThatDoNotFitTheStride();
        SetDataRejectsBadRanges();
        ReleasedBuffersRejectEveryMember();
        ReleaseAndDisposeAreTheSameAndRepeatable();
        Console.WriteLine("PASS: GraphicsBuffer validation, zeroed managed contents, unique handles, SetData offsets and strides, and release");
    }

    private static void ConstructorValidatesItsArguments()
    {
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer(Raw, 0, 4), "a zero count");
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer(Raw, -1, 4), "a negative count");
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer(Raw, 4, 0), "a zero stride");
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer(Raw, 4, -4), "a negative stride");
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer(Raw, int.MaxValue, 4), "a size beyond an int");
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer(Raw, 65536, 65536), "a size that overflows an int multiplication");
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer((GraphicsBuffer.Target)0, 4, 4), "no target");
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer((GraphicsBuffer.Target)(1 << 24), 4, 4), "an undefined target bit");
        Expect.Throws<ArgumentException>(() => new GraphicsBuffer(Raw | (GraphicsBuffer.Target)(1 << 24), 4, 4), "a defined target with an undefined bit");
    }

    private static void ReportsCountStrideAndZeroedContents()
    {
        using var buffer = new GraphicsBuffer(Raw, 8, 4);
        Expect.That(buffer.count == 8 && buffer.stride == 4, "count and stride");
        Expect.Equal(32, buffer.Contents.Length, "contents are count times stride bytes");
        Expect.That(!buffer.Contents.ContainsAnyExcept((byte)0), "contents start zeroed");
    }

    private static void HandlesAreUniqueAndNonZero()
    {
        var first = new GraphicsBuffer(Raw, 1, 4);
        var second = new GraphicsBuffer(Raw, 1, 4);
        Expect.That(first.bufferHandle.value != 0 && second.bufferHandle.value != 0, "a handle is never zero");
        Expect.That(first.bufferHandle != second.bufferHandle, "live buffers have different handles");
        var released = first.bufferHandle;
        first.Release();
        using var third = new GraphicsBuffer(Raw, 1, 4);
        Expect.That(third.bufferHandle != released && third.bufferHandle != second.bufferHandle, "a released handle is not handed out again");
        second.Dispose();
    }

    private static void SetDataCopiesBytesAtElementOffsets()
    {
        using var buffer = new GraphicsBuffer(Raw, 12, 4);
        buffer.SetData(new[] { 10, 11, 12, 13, 14 }, 1, 3, 2);
        Expect.Sequence(new[] { 0, 0, 0, 11, 12, 0, 0, 0, 0, 0, 0, 0 }, Ints(buffer), "ints land at element index 3 from managed index 1");

        // The buffer index counts elements of the array's own type, as the game lays out its instance data.
        buffer.SetData(new[] { new Wide { A = 1, B = 2, C = 3, D = 4 }, new Wide { A = 5, B = 6, C = 7, D = 8 } }, 1, 2, 1);
        Expect.Sequence(new[] { 0, 0, 0, 11, 12, 0, 0, 0, 5, 6, 7, 8 }, Ints(buffer), "a 16 byte element at element index 2 starts at byte 32");
        buffer.SetData(new int[] { 99 }, 0, 11, 0);
        Expect.Equal(8, Ints(buffer)[11], "a zero count copies nothing");
        buffer.SetData(new[] { 7 }, 0, 11, 1);
        Expect.Equal(7, Ints(buffer)[11], "the last element of the buffer is writable");
    }

    private static int[] Ints(GraphicsBuffer buffer) => MemoryMarshal.Cast<byte, int>(buffer.Contents).ToArray();

    private static void SetDataRejectsElementsThatDoNotFitTheStride()
    {
        using var buffer = new GraphicsBuffer(Raw, 8, 4);
        var mismatch = Expect.Catch<ArgumentException>(() => buffer.SetData(new short[4], 0, 0, 2), "a 2 byte element in a 4 byte stride buffer");
        Expect.That(mismatch.Message.Contains("(2 bytes)") && mismatch.Message.Contains("(4 bytes)"), "the message names both sizes: " + mismatch.Message);
        Expect.Throws<ArgumentException>(() => buffer.SetData(new byte[8], 0, 0, 4), "a 1 byte element");
        Expect.Throws<ArgumentException>(() => buffer.SetData(new Holder[1], 0, 0, 1), "an element that holds a reference");
        Expect.Throws<ArgumentException>(() => buffer.SetData(new string[1], 0, 0, 1), "a reference element");
        Expect.Throws<ArgumentNullException>(() => buffer.SetData(null!, 0, 0, 1), "no array");
        Expect.Equal(0, Ints(buffer).Sum(), "a rejected call writes nothing");
    }

    private static void SetDataRejectsBadRanges()
    {
        using var buffer = new GraphicsBuffer(Raw, 4, 4);
        var data = new[] { 1, 2, 3 };
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(data, -1, 0, 1), "a negative managed start");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(data, 0, -1, 1), "a negative buffer start");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(data, 0, 0, -1), "a negative count");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(data, 2, 0, 2), "a count past the end of the array");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(data, 4, 0, 0), "a managed start past the end of the array");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(data, 0, 2, 3), "a count past the end of the buffer");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(data, 0, 5, 0), "a buffer start past the end of the buffer");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(data, 0, int.MaxValue, 1), "a buffer start whose byte offset overflows");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.SetData(new Wide[2], 0, 1, 2), "wide elements past the end of the buffer");
        Expect.Equal(0, Ints(buffer).Sum(), "a rejected call writes nothing");
    }

    private static void ReleasedBuffersRejectEveryMember()
    {
        var buffer = new GraphicsBuffer(Raw, 2, 4);
        buffer.Release();
        Expect.Throws<ObjectDisposedException>(() => _ = buffer.count, "count after release");
        Expect.Throws<ObjectDisposedException>(() => _ = buffer.stride, "stride after release");
        Expect.Throws<ObjectDisposedException>(() => _ = buffer.bufferHandle, "bufferHandle after release");
        Expect.Throws<ObjectDisposedException>(() => buffer.SetData(new[] { 1 }, 0, 0, 1), "SetData after release");
        Expect.Throws<ObjectDisposedException>(() => _ = buffer.Contents.Length, "Contents after release");
    }

    private static void ReleaseAndDisposeAreTheSameAndRepeatable()
    {
        var buffer = new GraphicsBuffer(Raw, 2, 4);
        buffer.Release();
        buffer.Release();
        buffer.Dispose();
        var other = new GraphicsBuffer(Raw, 2, 4);
        other.Dispose();
        other.Dispose();
        Expect.Throws<ObjectDisposedException>(() => _ = other.count, "a disposed buffer is released");
    }
}
