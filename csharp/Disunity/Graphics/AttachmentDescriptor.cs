using UnityEngine.Experimental.Rendering;

namespace UnityEngine.Rendering;

/// <summary>
/// One attachment of a render pass: its format, what happens to it when the pass starts and ends, the targets it
/// loads from, stores to and resolves to, and its clear values. Plain data; the engine does not run render passes
/// yet. A new descriptor loads and stores nothing (<c>DontCare</c>), has invalid targets and clears to
/// transparent black, depth 1 and stencil 0. That default is the engine's choice.
/// </summary>
public struct AttachmentDescriptor : IEquatable<AttachmentDescriptor>
{
    public RenderBufferLoadAction loadAction { get; set; }
    public RenderBufferStoreAction storeAction { get; set; }
    public GraphicsFormat graphicsFormat { get; set; }
    public RenderTargetIdentifier loadStoreTarget { get; set; }
    public RenderTargetIdentifier resolveTarget { get; set; }
    public Color clearColor { get; set; }
    public float clearDepth { get; set; }
    public uint clearStencil { get; set; }

    public AttachmentDescriptor(GraphicsFormat format)
    {
        graphicsFormat = format;
        loadAction = RenderBufferLoadAction.DontCare;
        storeAction = RenderBufferStoreAction.DontCare;
        loadStoreTarget = RenderTargetIdentifier.Invalid;
        resolveTarget = RenderTargetIdentifier.Invalid;
        clearColor = Color.clear;
        clearDepth = 1f;
        clearStencil = 0;
    }

    /// <summary>The graphics format a render texture of this format has in the project's color space.</summary>
    public AttachmentDescriptor(RenderTextureFormat format) :
        this(GraphicsFormats.ColorFormat(format, GraphicsFormats.UsesSrgb(RenderTextureReadWrite.Default))) { }

    /// <summary>Sets the target. Loading existing contents sets the load action to Load unless it is Clear; storing sets Store, or StoreAndResolve after a resolve target; not storing undoes both.</summary>
    public void ConfigureTarget(RenderTargetIdentifier target, bool loadExistingContents, bool storeResults)
    {
        loadStoreTarget = target;
        if (loadExistingContents && loadAction != RenderBufferLoadAction.Clear) loadAction = RenderBufferLoadAction.Load;
        storeAction = (storeResults, storeAction) switch
        {
            (true, RenderBufferStoreAction.DontCare) => RenderBufferStoreAction.Store,
            (true, RenderBufferStoreAction.Resolve) => RenderBufferStoreAction.StoreAndResolve,
            (false, RenderBufferStoreAction.Store) => RenderBufferStoreAction.DontCare,
            (false, RenderBufferStoreAction.StoreAndResolve) => RenderBufferStoreAction.Resolve,
            _ => storeAction,
        };
    }

    /// <summary>Sets the resolve target; a store action of DontCare becomes Resolve and Store becomes StoreAndResolve.</summary>
    public void ConfigureResolveTarget(RenderTargetIdentifier target)
    {
        resolveTarget = target;
        storeAction = storeAction switch
        {
            RenderBufferStoreAction.DontCare => RenderBufferStoreAction.Resolve,
            RenderBufferStoreAction.Store => RenderBufferStoreAction.StoreAndResolve,
            _ => storeAction,
        };
    }

    /// <summary>Sets the clear values and the load action to Clear.</summary>
    public void ConfigureClear(Color clearColor, float clearDepth = 1f, uint clearStencil = 0)
    {
        this.clearColor = clearColor;
        this.clearDepth = clearDepth;
        this.clearStencil = clearStencil;
        loadAction = RenderBufferLoadAction.Clear;
    }

    public readonly bool Equals(AttachmentDescriptor other) =>
        loadAction == other.loadAction && storeAction == other.storeAction && graphicsFormat == other.graphicsFormat
        && loadStoreTarget.Equals(other.loadStoreTarget) && resolveTarget.Equals(other.resolveTarget)
        && clearColor.Equals(other.clearColor) && clearDepth.Equals(other.clearDepth) && clearStencil == other.clearStencil;

    public override readonly bool Equals(object? obj) => obj is AttachmentDescriptor other && Equals(other);

    public override readonly int GetHashCode() =>
        HashCode.Combine(loadAction, storeAction, graphicsFormat, loadStoreTarget, resolveTarget, clearColor, HashCode.Combine(clearDepth, clearStencil));

    public static bool operator ==(AttachmentDescriptor left, AttachmentDescriptor right) => left.Equals(right);
    public static bool operator !=(AttachmentDescriptor left, AttachmentDescriptor right) => !left.Equals(right);
}
