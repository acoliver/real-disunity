using Unity.Collections;

namespace UnityEngine;

/// <summary>
/// A GPU buffer. The engine has no GPU memory for it yet: the buffer keeps its size and its released
/// state, and reading or writing its contents throws <see cref="NotSupportedException"/>. Release and
/// Dispose are the same operation and may be repeated; <see cref="count"/> still reads afterwards.
/// </summary>
public sealed class ComputeBuffer : IDisposable
{
    private bool _released;

    public ComputeBuffer(int count, int stride) : this(count, stride, ComputeBufferType.Default) { }

    public ComputeBuffer(int count, int stride, ComputeBufferType type) : this(count, stride, type, ComputeBufferMode.Immutable) { }

    public ComputeBuffer(int count, int stride, ComputeBufferType type, ComputeBufferMode usage)
    {
        if (count <= 0) throw new ArgumentException("ComputeBuffer count must be greater than zero", nameof(count));
        if (stride <= 0) throw new ArgumentException("ComputeBuffer stride must be greater than zero", nameof(stride));
        this.count = count;
    }

    public int count { get; }

    public void Release() => _released = true;

    public void Dispose() => Release();

    public void GetData(Array data) => Contents(nameof(GetData));

    public void SetData(Array data) => Contents(nameof(SetData));

    public void SetData<T>(NativeArray<T> data, int nativeBufferStartIndex, int computeBufferStartIndex, int count) where T : struct =>
        Contents(nameof(SetData));

    private void Contents(string member)
    {
        ObjectDisposedException.ThrowIf(_released, this);
        throw new NotSupportedException($"ComputeBuffer.{member}: compute buffer contents are not supported yet");
    }
}
