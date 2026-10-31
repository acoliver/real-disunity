namespace UnityEngine;

/// <summary>
/// The render target calls that take a <see cref="RenderTexture"/>. As the engine's choice they only change
/// <see cref="RenderTexture.active"/>, since nothing is drawn into the texture: a texture becomes the active one (and is
/// created if it was not), and null makes the screen the target again. A mip level or depth slice the texture does not
/// have is an <see cref="ArgumentException"/>. Each argument count is its own overload, because a compiled game calls the
/// overload it was built against.
/// </summary>
public static partial class Graphics
{
    public static void SetRenderTarget(RenderTexture rt) => SetRenderTarget(rt, 0, CubemapFace.Unknown, 0);

    public static void SetRenderTarget(RenderTexture rt, int mipLevel) => SetRenderTarget(rt, mipLevel, CubemapFace.Unknown, 0);

    public static void SetRenderTarget(RenderTexture rt, int mipLevel, CubemapFace face) => SetRenderTarget(rt, mipLevel, face, 0);

    public static void SetRenderTarget(RenderTexture rt, int mipLevel, CubemapFace face, int depthSlice)
    {
        if (rt is { } target && target)
        {
            if (mipLevel < 0 || mipLevel >= target.mipmapCount)
                throw new ArgumentException($"Graphics.SetRenderTarget: mip level {mipLevel} is outside 0 to {target.mipmapCount - 1} of render texture '{target.name}'", nameof(mipLevel));
            if (depthSlice < 0 || depthSlice >= target.volumeDepth)
                throw new ArgumentException($"Graphics.SetRenderTarget: depth slice {depthSlice} is outside 0 to {target.volumeDepth - 1} of render texture '{target.name}'", nameof(depthSlice));
        }
        RenderTexture.active = rt ? rt : null;
    }
}
