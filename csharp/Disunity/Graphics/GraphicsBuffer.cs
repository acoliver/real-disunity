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
    /// <summary>What the buffer is used for. The engine accepts only the targets the games it runs use.</summary>
    [Flags]
    public enum Target
    {
        Raw = 32,
    }

    private const Target Supported = Target.Raw;

    private static readonly ConcurrentDictionary<uint, WeakReference<GraphicsBuffer>> Live = new();
    private static long s_lastHandle;

    private readonly int _count;
    private readonly int _stride;
    private readonly GraphicsBufferHandle _handle;
    private byte[]? _contents;

    public GraphicsBuffer(Target target, int count, int stride)
    {
        if (target == 0 || (target & ~Supported) != 0)
            throw new ArgumentException($"GraphicsBuffer target {(int)target} is not supported; the engine supports Target.Raw", nameof(target));
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
        ref var first = ref MemoryMarshal.GetArrayDataReference(data);
        var source = MemoryMarshal.CreateReadOnlySpan(ref Unsafe.Add(ref first, (nint)managedBufferStartIndex * size), count * size);
        source.CopyTo(contents.AsSpan(graphicsBufferStartIndex * size, count * size));
    }

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
