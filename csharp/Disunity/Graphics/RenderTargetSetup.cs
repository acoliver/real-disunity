using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The buffers to render into, with the mip level, cube face and depth slice to use and what to do with
/// each buffer's contents. The struct stores them; <see cref="Graphics.SetRenderTarget(RenderTargetSetup)"/> does not bind them yet.
/// Every buffer loads and stores by default.
/// </summary>
public struct RenderTargetSetup
{
    public RenderBuffer[] color;
    public RenderBuffer depth;
    public int mipLevel;
    public CubemapFace cubemapFace;
    public int depthSlice;
    public RenderBufferLoadAction[] colorLoad;
    public RenderBufferStoreAction[] colorStore;
    public RenderBufferLoadAction depthLoad;
    public RenderBufferStoreAction depthStore;

    public RenderTargetSetup(RenderBuffer[] color, RenderBuffer depth, int mip, CubemapFace face, int depthSlice)
    {
        ArgumentNullException.ThrowIfNull(color);
        this.color = color;
        this.depth = depth;
        mipLevel = mip;
        cubemapFace = face;
        this.depthSlice = depthSlice;
        colorLoad = new RenderBufferLoadAction[color.Length];
        colorStore = new RenderBufferStoreAction[color.Length];
        Array.Fill(colorLoad, RenderBufferLoadAction.Load);
        Array.Fill(colorStore, RenderBufferStoreAction.Store);
        depthLoad = RenderBufferLoadAction.Load;
        depthStore = RenderBufferStoreAction.Store;
    }

    public RenderTargetSetup(RenderBuffer[] color, RenderBuffer depth, int mip, CubemapFace face) : this(color, depth, mip, face, 0) { }

    public RenderTargetSetup(RenderBuffer[] color, RenderBuffer depth, int mip) : this(color, depth, mip, CubemapFace.Unknown, 0) { }

    public RenderTargetSetup(RenderBuffer[] color, RenderBuffer depth) : this(color, depth, 0, CubemapFace.Unknown, 0) { }

    public RenderTargetSetup(RenderBuffer color, RenderBuffer depth, int mip, CubemapFace face, int depthSlice)
        : this(new[] { color }, depth, mip, face, depthSlice) { }

    public RenderTargetSetup(RenderBuffer color, RenderBuffer depth, int mip, CubemapFace face) : this(new[] { color }, depth, mip, face, 0) { }

    public RenderTargetSetup(RenderBuffer color, RenderBuffer depth, int mip) : this(new[] { color }, depth, mip, CubemapFace.Unknown, 0) { }

    public RenderTargetSetup(RenderBuffer color, RenderBuffer depth) : this(new[] { color }, depth, 0, CubemapFace.Unknown, 0) { }
}
