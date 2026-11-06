using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// A request to read GPU memory back to the CPU. The engine reads nothing back, so a request holds no state and
/// every member throws <see cref="NotSupportedException"/>.
/// </summary>
public struct AsyncGPUReadbackRequest
{
    private static NotSupportedException Unsupported(string member) => new($"AsyncGPUReadbackRequest.{member} is not supported: the engine reads nothing back from the GPU");

    public bool done => throw Unsupported(nameof(done));
    public bool hasError => throw Unsupported(nameof(hasError));
    public int layerCount => throw Unsupported(nameof(layerCount));
    public int layerDataSize => throw Unsupported(nameof(layerDataSize));
    public int width => throw Unsupported(nameof(width));
    public int height => throw Unsupported(nameof(height));
    public int depth => throw Unsupported(nameof(depth));

    public void Update() => throw Unsupported(nameof(Update));
    public void WaitForCompletion() => throw Unsupported(nameof(WaitForCompletion));
    public NativeArray<T> GetData<T>(int layer = 0) where T : struct => throw Unsupported(nameof(GetData));
}
