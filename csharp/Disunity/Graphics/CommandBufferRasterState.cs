namespace UnityEngine.Rendering;

/// <summary>
/// The global depth bias a shadow caster pass sets around its draws. The engine's offscreen shadow draws apply no global depth
/// bias yet, so the member records nothing and the caster draws are made without it. Applying it needs a command on the offscreen list and a depth
/// bias in the draw state the host builds for a shadow list.
/// </summary>
public sealed partial class CommandBuffer
{
    /// <summary>Accepts a constant and a slope-scaled depth bias for the draws that follow; a released buffer throws
    /// <see cref="ObjectDisposedException"/>.</summary>
    public void SetGlobalDepthBias(float bias, float slopeBias) => ThrowIfReleased();
}
