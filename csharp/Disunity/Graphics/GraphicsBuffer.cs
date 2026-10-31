using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Unity.Collections.LowLevel.Unsafe;

namespace UnityEngine;

/// <summary>
/// A buffer of GPU data. The engine has no GPU buffers yet, so the contents live in managed memory
/// (count times stride bytes, zeroed) where a renderer can read them. Each live buffer has its own
/// <see cref="bufferHandle"/>, which <c>BatchRendererGroup.AddBatch</c> resolves back to the buffer.
/// </summary>
public sealed class GraphicsBuffer : IDisposable
{
    /// <summary>What the buffer is used for. The contents are managed memory whatever the target, so every
    /// target is accepted; the targets matter once a renderer reads the buffer.</summary>
    [Flags]
    public enum Target
    {
        Vertex = 1,
        Index = 2,
        CopySource = 4,
        CopyDestination = 8,
        Structured = 16,
        Raw = 32,
        Append = 64,
        Counter = 128,
        IndirectArguments = 256,
        Constant = 512,
    }

    /// <summary>How the buffer's contents are updated. Stored; the managed contents behave the same either way.</summary>
    [Flags]
    public enum UsageFlags
    {
        None = 0,
        LockBufferForWrite = 1,
    }

    private const Target Supported = (Target)1023;

    private static readonly ConcurrentDictionary<uint, WeakReference<GraphicsBuffer>> Live = new();
    private static long s_lastHandle;

    private readonly Target _target;
    private readonly UsageFlags _usage;
    private readonly int _count;
    private readonly int _stride;
    private readonly GraphicsBufferHandle _handle;
    private byte[]? _contents;

    public GraphicsBuffer(Target target, int count, int stride)
        : this(target, UsageFlags.None, count, stride)
    {
    }

    public GraphicsBuffer(Target target, UsageFlags usageFlags, int count, int stride)
    {
        if (target == 0 || (target & ~Supported) != 0)
            throw new ArgumentException($"GraphicsBuffer target {(int)target} is not a combination of the defined targets", nameof(target));
        if ((usageFlags & ~UsageFlags.LockBufferForWrite) != 0)
            throw new ArgumentException($"GraphicsBuffer usage flags {(int)usageFlags} are not defined", nameof(usageFlags));
        _target = target;
        _usage = usageFlags;
        if (count <= 0) throw new ArgumentException($"GraphicsBuffer count must be positive, got {count}", nameof(count));
        if (stride <= 0) throw new ArgumentException($"GraphicsBuffer stride must be positive, got {stride}", nameof(stride));
        var bytes = (long)count * stride;
        if (bytes > int.MaxValue)
            throw new ArgumentException($"GraphicsBuffer of {count} elements of {stride} bytes is {bytes} bytes, more than the engine's limit of {int.MaxValue}", nameof(count));
        _count = count;
        _stride = stride;
        _contents = new byte[bytes];
        _handle = new GraphicsBufferHandle { value = NextHandle() };
        Live[_handle.value] = new WeakReference<GraphicsBuffer>(this);
    }

    ~GraphicsBuffer() => Release();

    public int count => WhileLive(_count);
    public int stride => WhileLive(_stride);
    public GraphicsBufferHandle bufferHandle => WhileLive(_handle);
    public Target target => WhileLive(_target);
    public UsageFlags usageFlags => WhileLive(_usage);

    /// <summary>A name for tools and logs. It can be read and set after release, as it describes the object.</summary>
    public string name { get; set; } = "";

    /// <summary>True until the buffer is released.</summary>
    public bool IsValid() => _contents != null;

    /// <summary>The buffer's bytes, for a renderer. Throws once the buffer is released.</summary>
    internal ReadOnlySpan<byte> Contents => Storage();

    /// <summary>True when the handle belongs to a buffer that has not been released.</summary>
    internal static bool IsLive(GraphicsBufferHandle handle) =>
        Live.TryGetValue(handle.value, out var buffer) && buffer.TryGetTarget(out var target) && target._contents != null;

    /// <summary>
    /// Copies <paramref name="count"/> elements of <paramref name="data"/> into the buffer. The element size must
    /// be a multiple of the stride, and the buffer index counts elements of the array's own type, which is how
    /// the game addresses the sections of its instance data.
    /// </summary>
    public void SetData(Array data, int managedBufferStartIndex, int graphicsBufferStartIndex, int count)
    {
        var contents = Storage();
        ArgumentNullException.ThrowIfNull(data);
        var size = ElementSize(data.GetType().GetElementType()!);
        CheckRange(data.Length, contents.Length, size, managedBufferStartIndex, graphicsBufferStartIndex, count);
        if (count == 0) return;
        ArrayBytes(data, size).Slice(managedBufferStartIndex * size, count * size).CopyTo(contents.AsSpan(graphicsBufferStartIndex * size, count * size));
    }

    /// <summary>Copies the whole array to the start of the buffer.</summary>
    public void SetData(Array data)
    {
        ArgumentNullException.ThrowIfNull(data);
        SetData(data, 0, 0, data.Length);
    }

    public void SetData<T>(List<T> data) where T : struct
    {
        ArgumentNullException.ThrowIfNull(data);
        SetData(data, 0, 0, data.Count);
    }

    public void SetData<T>(List<T> data, int managedBufferStartIndex, int graphicsBufferStartIndex, int count) where T : struct
    {
        ArgumentNullException.ThrowIfNull(data);
        CopyIn<T>(CollectionsMarshal.AsSpan(data), managedBufferStartIndex, graphicsBufferStartIndex, count);
    }

    public void SetData<T>(Unity.Collections.NativeArray<T> data) where T : struct =>
        SetData(data, 0, 0, data.Length);

    public void SetData<T>(Unity.Collections.NativeArray<T> data, int nativeBufferStartIndex, int graphicsBufferStartIndex, int count) where T : struct =>
        CopyIn<T>(data.AsReadOnlySpan(), nativeBufferStartIndex, graphicsBufferStartIndex, count);

    /// <summary>Copies the start of the buffer into the whole array.</summary>
    public void GetData(Array data)
    {
        ArgumentNullException.ThrowIfNull(data);
        GetData(data, 0, 0, data.Length);
    }

    /// <summary>Copies <paramref name="count"/> elements from the buffer into <paramref name="data"/>; the buffer
    /// index counts elements of the array's own type, as <see cref="SetData(Array, int, int, int)"/> does.</summary>
    public void GetData(Array data, int managedBufferStartIndex, int computeBufferStartIndex, int count)
    {
        var contents = Storage();
        ArgumentNullException.ThrowIfNull(data);
        var size = ElementSize(data.GetType().GetElementType()!);
        CheckRange(data.Length, contents.Length, size, managedBufferStartIndex, computeBufferStartIndex, count);
        if (count == 0) return;
        contents.AsSpan(computeBufferStartIndex * size, count * size).CopyTo(ArrayBytes(data, size).Slice(managedBufferStartIndex * size, count * size));
    }

    private void CopyIn<T>(ReadOnlySpan<T> data, int managedStart, int bufferStart, int count) where T : struct
    {
        var contents = Storage();
        var size = ElementSize(typeof(T));
        CheckRange(data.Length, contents.Length, size, managedStart, bufferStart, count);
        if (count == 0) return;
        MemoryMarshal.AsBytes(data.Slice(managedStart, count)).CopyTo(contents.AsSpan(bufferStart * size, count * size));
    }

    private static Span<byte> ArrayBytes(Array data, int size) =>
        MemoryMarshal.CreateSpan(ref MemoryMarshal.GetArrayDataReference(data), data.Length * size);

    public void Release()
    {
        var contents = Interlocked.Exchange(ref _contents, null);
        if (contents == null) return;
        Live.TryRemove(_handle.value, out _);
        GC.SuppressFinalize(this);
    }

    public void Dispose() => Release();

    private byte[] Storage() => _contents ?? throw new ObjectDisposedException(nameof(GraphicsBuffer), "the GraphicsBuffer has been released");

    private T WhileLive<T>(T value)
    {
        _ = Storage();
        return value;
    }

    private static uint NextHandle()
    {
        while (true)
        {
            var next = (uint)Interlocked.Increment(ref s_lastHandle);
            if (next != 0) return next;
        }
    }

    private int ElementSize(Type element)
    {
        if (!UnsafeUtility.IsUnmanaged(element))
            throw new ArgumentException($"SetData needs an array of unmanaged elements, not {element}", "data");
        var size = UnsafeUtility.SizeOf(element);
        if (size % _stride != 0)
            throw new ArgumentException($"the element size ({size} bytes) is not a multiple of the buffer stride ({_stride} bytes)", "data");
        return size;
    }

    private static void CheckRange(int arrayLength, int bufferBytes, int size, int managedStart, int bufferStart, int count)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(managedStart);
        ArgumentOutOfRangeException.ThrowIfNegative(bufferStart);
        ArgumentOutOfRangeException.ThrowIfNegative(count);
        if ((long)managedStart + count > arrayLength)
            throw new ArgumentOutOfRangeException(nameof(count), $"{count} elements from index {managedStart} are past the end of an array of {arrayLength}");
        if (((long)bufferStart + count) * size > bufferBytes)
            throw new ArgumentOutOfRangeException(nameof(count), $"{count} elements of {size} bytes from buffer index {bufferStart} are past the end of the {bufferBytes} byte buffer");
    }
}
