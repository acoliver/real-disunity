using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// Engine-wide graphics values and the immediate rendering calls. The values describe a machine with the
/// top hardware tier, an sRGB display, no OpenGL ES requirement and no framebuffer alpha; only
/// <see cref="activeTier"/> can be set, and it stores the value without changing anything. The engine draws through
/// render-plan and render-scene, so every call that would draw, copy or run GPU work throws
/// <see cref="NotSupportedException"/> naming the member. The copy, target and synchronisation calls are here; the
/// mesh and procedural draws are in GraphicsDraw.cs and the <see cref="RenderParams"/> draws in GraphicsRender.cs.
/// </summary>
public static partial class Graphics
{
    private static GraphicsTier _activeTier = GraphicsTier.Tier3;

    public static GraphicsTier activeTier
    {
        get => _activeTier;
        set => _activeTier = value;
    }

    public static OpenGLESVersion minOpenGLESVersion => OpenGLESVersion.None;

    public static bool preserveFramebufferAlpha => false;

    public static ColorGamut activeColorGamut => ColorGamut.sRGB;

    /// <summary>No render target is ever bound, so this is the default buffer.</summary>
    public static RenderBuffer activeColorBuffer => default;

    /// <summary>No render target is ever bound, so this is the default buffer.</summary>
    public static RenderBuffer activeDepthBuffer => default;

    private static NotSupportedException Unsupported(string member) =>
        new($"Graphics.{member}: the engine draws through render-plan and render-scene; executing this call is not supported yet");

    public static void Blit(Texture source, RenderTexture dest) => throw Unsupported(nameof(Blit));
    public static void Blit(Texture source, RenderTexture dest, Vector2 scale, Vector2 offset) => throw Unsupported(nameof(Blit));

    public static void Blit(Texture source, RenderTexture dest, Vector2 scale, Vector2 offset, int sourceDepthSlice, int destDepthSlice) =>
        throw Unsupported(nameof(Blit));

    public static void Blit(Texture source, RenderTexture dest, Material mat, int pass = -1) => throw Unsupported(nameof(Blit));
    public static void Blit(Texture source, RenderTexture dest, Material mat, int pass, int destDepthSlice) => throw Unsupported(nameof(Blit));
    public static void Blit(Texture source, Material mat, int pass = -1) => throw Unsupported(nameof(Blit));
    public static void Blit(Texture source, Material mat, int pass, int destDepthSlice) => throw Unsupported(nameof(Blit));

    public static void BlitMultiTap(Texture source, RenderTexture dest, Material mat, params Vector2[] offsets) =>
        throw Unsupported(nameof(BlitMultiTap));

    public static void BlitMultiTap(Texture source, RenderTexture dest, Material mat, int destDepthSlice, params Vector2[] offsets) =>
        throw Unsupported(nameof(BlitMultiTap));

    public static void CopyTexture(Texture src, Texture dst) => throw Unsupported(nameof(CopyTexture));
    public static void CopyTexture(Texture src, int srcElement, Texture dst, int dstElement) => throw Unsupported(nameof(CopyTexture));

    public static void CopyTexture(Texture src, int srcElement, int srcMip, Texture dst, int dstElement, int dstMip) =>
        throw Unsupported(nameof(CopyTexture));

    public static void CopyTexture(Texture src, int srcElement, int srcMip, int srcX, int srcY, int srcWidth, int srcHeight, Texture dst,
        int dstElement, int dstMip, int dstX, int dstY) => throw Unsupported(nameof(CopyTexture));

    public static bool ConvertTexture(Texture src, Texture dst) => throw Unsupported(nameof(ConvertTexture));
    public static bool ConvertTexture(Texture src, int srcElement, Texture dst, int dstElement) => throw Unsupported(nameof(ConvertTexture));

    public static void CopyBuffer(GraphicsBuffer source, GraphicsBuffer dest) => throw Unsupported(nameof(CopyBuffer));

    public static void DrawTexture(Rect screenRect, Texture texture, Material? mat = null, int pass = -1) => throw Unsupported(nameof(DrawTexture));

    public static void DrawTexture(Rect screenRect, Texture texture, int leftBorder, int rightBorder, int topBorder, int bottomBorder,
        Material? mat = null, int pass = -1) => throw Unsupported(nameof(DrawTexture));

    public static void DrawTexture(Rect screenRect, Texture texture, Rect sourceRect, int leftBorder, int rightBorder, int topBorder,
        int bottomBorder, Material? mat = null, int pass = -1) => throw Unsupported(nameof(DrawTexture));

    public static void DrawTexture(Rect screenRect, Texture texture, int leftBorder, int rightBorder, int topBorder, int bottomBorder,
        Color color, Material? mat = null, int pass = -1) => throw Unsupported(nameof(DrawTexture));

    public static void DrawTexture(Rect screenRect, Texture texture, Rect sourceRect, int leftBorder, int rightBorder, int topBorder,
        int bottomBorder, Color color, Material? mat = null, int pass = -1) => throw Unsupported(nameof(DrawTexture));

    public static void SetRenderTarget(RenderTexture rt, int mipLevel = 0, CubemapFace face = CubemapFace.Unknown, int depthSlice = 0) =>
        throw Unsupported(nameof(SetRenderTarget));

    public static void SetRenderTarget(RenderBuffer colorBuffer, RenderBuffer depthBuffer, int mipLevel = 0,
        CubemapFace face = CubemapFace.Unknown, int depthSlice = 0) => throw Unsupported(nameof(SetRenderTarget));

    public static void SetRenderTarget(RenderBuffer[] colorBuffers, RenderBuffer depthBuffer) => throw Unsupported(nameof(SetRenderTarget));
    public static void SetRenderTarget(RenderTargetSetup setup) => throw Unsupported(nameof(SetRenderTarget));

    public static void SetRandomWriteTarget(int index, RenderTexture uav) => throw Unsupported(nameof(SetRandomWriteTarget));

    public static void SetRandomWriteTarget(int index, ComputeBuffer uav, bool preserveCounterValue = false) =>
        throw Unsupported(nameof(SetRandomWriteTarget));

    public static void SetRandomWriteTarget(int index, GraphicsBuffer uav, bool preserveCounterValue = false) =>
        throw Unsupported(nameof(SetRandomWriteTarget));

    public static void ClearRandomWriteTargets() => throw Unsupported(nameof(ClearRandomWriteTargets));

    public static void ExecuteCommandBuffer(CommandBuffer buffer) => throw Unsupported(nameof(ExecuteCommandBuffer));

    public static void ExecuteCommandBufferAsync(CommandBuffer buffer, ComputeQueueType queueType) =>
        throw Unsupported(nameof(ExecuteCommandBufferAsync));

    public static GraphicsFence CreateGraphicsFence(GraphicsFenceType fenceType,
        SynchronisationStageFlags stage = SynchronisationStageFlags.PixelProcessing) => throw Unsupported(nameof(CreateGraphicsFence));

    public static GraphicsFence CreateAsyncGraphicsFence(SynchronisationStage stage = SynchronisationStage.PixelProcessing) =>
        throw Unsupported(nameof(CreateAsyncGraphicsFence));

    public static void WaitOnAsyncGraphicsFence(GraphicsFence fence, SynchronisationStage stage = SynchronisationStage.PixelProcessing) =>
        throw Unsupported(nameof(WaitOnAsyncGraphicsFence));
}
