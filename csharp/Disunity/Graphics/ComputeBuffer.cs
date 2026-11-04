using System.Runtime.InteropServices;
using Disunity.Hosting;
using Unity.Collections;

namespace UnityEngine;

/// <summary>
/// A buffer of <see cref="count"/> elements of <see cref="stride"/> bytes whose contents the world holds. The world has a
/// compute buffer for it from its first use, which checks the shape (a stride that is a multiple of 4 from 4 to 2048, and at
/// most 256 MiB). <see cref="SetData(Array)"/> writes the bytes of the elements it is given. <see cref="GetData(Array)"/>
/// first asks the host to execute the recorded offscreen work, the dispatches included (a GPU flush), then copies the bytes
/// out; without a host executor it returns what the CPU wrote, zeros for what it did not. A call whose elements do not fill
/// the stride (an element type of another size) throws <see cref="ArgumentException"/>. Release and Dispose are the same
/// operation and may be repeated; <see cref="count"/> and <see cref="stride"/> still read afterwards, and
/// <see cref="IsValid"/> is false. The contents are sent to the world when they are set, so a world unloaded and loaded again
/// starts with a new, zeroed buffer.
/// </summary>
public sealed class ComputeBuffer : IDisposable
{
    private readonly ComputeBufferType _type;
    private WorldGraphics.Link _world;
    private bool _released;

    public ComputeBuffer(int count, int stride) : this(count, stride, ComputeBufferType.Default) { }

    public ComputeBuffer(int count, int stride, ComputeBufferType type) : this(count, stride, type, ComputeBufferMode.Immutable) { }

    public ComputeBuffer(int count, int stride, ComputeBufferType type, ComputeBufferMode usage)
    {
        if (count <= 0) throw new ArgumentException("ComputeBuffer count must be greater than zero", nameof(count));
        if (stride <= 0) throw new ArgumentException("ComputeBuffer stride must be greater than zero", nameof(stride));
        (this.count, this.stride, _type) = (count, stride, type);
    }

    public int count { get; }

    public int stride { get; }

    /// <summary>Whether the buffer can be used: true until it is released.</summary>
    public bool IsValid() => !_released;

    /// <summary>The buffer's resource in the loaded world, made on first use.</summary>
    internal WorldGraphics.Link WorldLink
    {
        get
        {
            ObjectDisposedException.ThrowIf(_released, this);
            if (_world.IsCurrent) return _world;
            try
            {
                _world = WorldCompute.CreateBuffer(count, stride, (int)_type);
            }
            catch (DisunityException error)
            {
                throw new ArgumentException($"ComputeBuffer: {error.Message}", nameof(stride), error);
            }
            return _world;
        }
    }

    public void Release()
    {
        if (_released) return;
        _released = true;
        if (_world.IsCurrent) WorldCompute.DestroyBuffer(_world);
    }

    public void Dispose() => Release();

    /// <summary>Copies the buffer's contents into <paramref name="data"/> after a GPU flush, as many elements as the array holds.</summary>
    public void GetData(Array data) => GetData(data, 0, 0, data?.Length ?? 0);

    /// <summary>Copies <paramref name="count"/> elements from element <paramref name="computeBufferStartIndex"/> of the buffer into
    /// <paramref name="data"/> from element <paramref name="managedBufferStartIndex"/>, after a GPU flush.</summary>
    public void GetData(Array data, int managedBufferStartIndex, int computeBufferStartIndex, int count)
    {
        ArgumentNullException.ThrowIfNull(data);
        var link = WorldLink;
        var (size, start, bytes) = Range(data, managedBufferStartIndex, computeBufferStartIndex, count, nameof(GetData));
        WorldCompute.Flush();
        var handle = GCHandle.Alloc(data, GCHandleType.Pinned);
        try
        {
            var span = Pinned(handle, size, managedBufferStartIndex, bytes);
            WorldCompute.ReadBuffer(link, start, span);
        }
        finally { handle.Free(); }
    }

    /// <summary>Writes the elements of <paramref name="data"/> to the start of the buffer.</summary>
    public void SetData(Array data) => SetData(data, 0, 0, data?.Length ?? 0);

    public void SetData<T>(List<T> data) where T : struct
    {
        ArgumentNullException.ThrowIfNull(data);
        SetData(data, 0, 0, data.Count);
    }

    public void SetData<T>(List<T> data, int managedBufferStartIndex, int computeBufferStartIndex, int count) where T : struct
    {
        ArgumentNullException.ThrowIfNull(data);
        var elements = new T[count < 0 ? 0 : count];
        if (managedBufferStartIndex < 0 || count < 0 || managedBufferStartIndex + count > data.Count)
            throw new ArgumentOutOfRangeException(nameof(count), "ComputeBuffer.SetData: the range lies outside the list");
        data.CopyTo(managedBufferStartIndex, elements, 0, count);
        SetData(elements, 0, computeBufferStartIndex, count);
    }

    public void SetData<T>(NativeArray<T> data) where T : struct => SetData(data, 0, 0, data.Length);

    public void SetData<T>(NativeArray<T> data, int nativeBufferStartIndex, int computeBufferStartIndex, int count) where T : struct
    {
        if (nativeBufferStartIndex < 0 || count < 0 || nativeBufferStartIndex + count > data.Length)
            throw new ArgumentOutOfRangeException(nameof(count), "ComputeBuffer.SetData: the range lies outside the native array");
        var elements = new T[count];
        for (var i = 0; i < count; i++) elements[i] = data[nativeBufferStartIndex + i];
        SetData(elements, 0, computeBufferStartIndex, count);
    }

    /// <summary>Writes <paramref name="count"/> elements of <paramref name="data"/> from element <paramref name="managedBufferStartIndex"/>
    /// to element <paramref name="computeBufferStartIndex"/> of the buffer.</summary>
    public void SetData(Array data, int managedBufferStartIndex, int computeBufferStartIndex, int count)
    {
        ArgumentNullException.ThrowIfNull(data);
        var link = WorldLink;
        var (size, start, bytes) = Range(data, managedBufferStartIndex, computeBufferStartIndex, count, nameof(SetData));
        var handle = GCHandle.Alloc(data, GCHandleType.Pinned);
        try
        {
            var span = Pinned(handle, size, managedBufferStartIndex, bytes);
            WorldCompute.WriteBuffer(link, start, span);
        }
        finally { handle.Free(); }
    }

    /// <summary>The byte size of an element, the byte offset in the buffer and the byte length of a range, after the checks every
    /// overload shares.</summary>
    private (int ElementSize, long Start, int Bytes) Range(Array data, int managedStart, int computeStart, int elements, string member)
    {
        var elementSize = ElementSize(data, member);
        if (managedStart < 0 || elements < 0 || computeStart < 0 || managedStart + elements > data.Length)
            throw new ArgumentOutOfRangeException(nameof(elements), $"ComputeBuffer.{member}: the range lies outside the array");
        if (computeStart + (long)elements > count)
            throw new ArgumentException($"ComputeBuffer.{member}: {elements} elements from element {computeStart} do not fit a buffer of {count}", nameof(elements));
        if (elementSize != stride)
            throw new ArgumentException($"ComputeBuffer.{member}: the array's elements are {elementSize} bytes and the buffer's stride is {stride}", nameof(data));
        return (elementSize, (long)computeStart * stride, elements * elementSize);
    }

    private static int ElementSize(Array data, string member)
    {
        var type = data.GetType().GetElementType()!;
        try
        {
            return Marshal.SizeOf(type);
        }
        catch (ArgumentException)
        {
            throw new ArgumentException($"ComputeBuffer.{member}: {type.Name} has no fixed size to copy", nameof(data));
        }
    }

    private static unsafe Span<byte> Pinned(GCHandle handle, int elementSize, int firstElement, int bytes) =>
        new((byte*)handle.AddrOfPinnedObject() + (long)firstElement * elementSize, bytes);
}
