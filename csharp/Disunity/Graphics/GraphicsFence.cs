namespace UnityEngine.Rendering;

/// <summary>
/// A point in the GPU's work that can be waited on. The engine runs a frame's work in order on one queue, so every
/// fence has passed: a command buffer makes one already passed, and a default one has passed too.
/// </summary>
public struct GraphicsFence
{
    public readonly GraphicsFenceType fenceType => GraphicsFenceType.AsyncQueueSynchronisation;

    public readonly bool passed => true;
}
