namespace UnityEngine.Rendering;

/// <summary>
/// A point in the GPU's work that can be waited on. The engine runs no GPU work that a script can fence, so a
/// fence is never created and a default one has passed.
/// </summary>
public struct GraphicsFence
{
    public readonly GraphicsFenceType fenceType => GraphicsFenceType.AsyncQueueSynchronisation;

    public readonly bool passed => true;
}
