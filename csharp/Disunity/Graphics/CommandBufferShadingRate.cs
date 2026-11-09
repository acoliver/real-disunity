namespace UnityEngine.Rendering;

/// <summary>
/// Variable-rate shading on a command buffer. A coarser shading rate only saves work: the engine shades every pixel at the
/// full rate, which draws the same image at full detail, so these members record nothing. A render graph resets the rate
/// around its passes whether or not the device supports variable rates.
/// </summary>
public sealed partial class CommandBuffer
{
    public void SetShadingRateFragmentSize(ShadingRateFragmentSize shadingRateFragmentSize) => ThrowIfReleased();

    public void SetShadingRateCombiner(ShadingRateCombinerStage stage, ShadingRateCombiner combiner) => ThrowIfReleased();

    public void SetShadingRateImage(in RenderTargetIdentifier shadingRateImage) => ThrowIfReleased();

    public void ResetShadingRate() => ThrowIfReleased();
}
