namespace UnityEngine.Rendering;

/// <summary>
/// Fences on a command buffer. The engine runs a frame's command buffers in the order they were executed, on one queue, so
/// work after a fence always sees the work before it: a fence is made already passed and waiting on one records nothing.
/// A render graph's native pass compiler waits on fences between its passes.
/// </summary>
public sealed partial class CommandBuffer
{
    public GraphicsFence CreateAsyncGraphicsFence() => Fence();

    public GraphicsFence CreateAsyncGraphicsFence(SynchronisationStage stage) => Fence();

    public GraphicsFence CreateGraphicsFence(GraphicsFenceType fenceType, SynchronisationStageFlags stage) => Fence();

    public void WaitOnAsyncGraphicsFence(GraphicsFence fence) => ThrowIfReleased();

    public void WaitOnAsyncGraphicsFence(GraphicsFence fence, SynchronisationStage stage) => ThrowIfReleased();

    public void WaitOnAsyncGraphicsFence(GraphicsFence fence, SynchronisationStageFlags stage) => ThrowIfReleased();

    private GraphicsFence Fence()
    {
        ThrowIfReleased();
        return default;
    }
}
