namespace UnityEngine.Rendering;

/// <summary>
/// The scissor rectangle and the global depth bias a shadow caster pass sets around its draws. Each call is a recorded command:
/// the host clips the draws that follow to the rectangle and gives the draws that follow the bias, in the order the buffer holds
/// them with its other commands.
/// </summary>
public sealed partial class CommandBuffer
{
    /// <summary>Records the rectangle, in pixels of the bound targets from the bottom left, that clips the draws after it until
    /// <see cref="DisableScissorRect"/>, a <see cref="SetRenderTarget(RenderTargetIdentifier)"/> or a camera setup. A component
    /// that is not finite or a negative width or height throws <see cref="ArgumentException"/> naming
    /// <c>CommandBuffer.EnableScissorRect</c> and records nothing.</summary>
    public void EnableScissorRect(Rect scissor)
    {
        ThrowIfReleased();
        if (!float.IsFinite(scissor.x) || !float.IsFinite(scissor.y) || !float.IsFinite(scissor.width) || !float.IsFinite(scissor.height)
            || scissor.width < 0f || scissor.height < 0f)
            throw new ArgumentException($"CommandBuffer.EnableScissorRect: the rectangle {scissor} needs finite components and a width and height of 0 or more", nameof(scissor));
        Record(() => WorldGraphics.RecordEnableScissor(scissor));
    }

    /// <summary>Records the end of the clip <see cref="EnableScissorRect"/> set: the draws after it cover the whole target.</summary>
    public void DisableScissorRect()
    {
        ThrowIfReleased();
        Record(WorldGraphics.RecordDisableScissor);
    }

    /// <summary>Records a constant and a slope-scaled depth bias for the draws that follow, until the next call; a bias of 0 and a
    /// slope bias of 0 end it. The host applies the two values as the constant and slope-scale terms of the pipeline's depth bias
    /// as given. A value that is not finite throws <see cref="ArgumentException"/> naming <c>CommandBuffer.SetGlobalDepthBias</c>
    /// and records nothing.</summary>
    public void SetGlobalDepthBias(float bias, float slopeBias)
    {
        ThrowIfReleased();
        if (!float.IsFinite(bias) || !float.IsFinite(slopeBias))
            throw new ArgumentException($"CommandBuffer.SetGlobalDepthBias: the bias {bias} and slope bias {slopeBias} must be finite", nameof(bias));
        Record(() => WorldGraphics.RecordDepthBias(bias, slopeBias));
    }
}
