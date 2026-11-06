using Disunity.Native;
using UnityEngine.Experimental.Rendering;

namespace UnityEngine.Rendering;

/// <summary>
/// The recording members that allocate and release temporary render textures by property name id. An allocation records the texture's
/// description in the world's units: the format, depth bits and mip count of a <see cref="RenderTexture"/> built from the same
/// arguments, the filter mode as the sampler's filter and clamped wrapping. A width or height of zero or below is the size of the
/// screen when the buffer is executed, which is the engine's choice for a texture sized to the camera. Anti-aliasing, random write
/// access, memoryless mode and dynamic scaling are accepted and have no effect.
/// </summary>
public sealed partial class CommandBuffer
{
    public void GetTemporaryRT(int nameID, RenderTextureDescriptor desc, FilterMode filter)
    {
        ThrowIfReleased();
        Record(() => WorldGraphics.RecordAllocateTemporary(nameID, Describe(desc, filter)));
    }

    public void GetTemporaryRT(int nameID, RenderTextureDescriptor desc) => GetTemporaryRT(nameID, desc, FilterMode.Point);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing, bool enableRandomWrite, RenderTextureMemoryless memorylessMode, bool useDynamicScale) =>
        GetTemporaryRT(nameID, new RenderTextureDescriptor(width, height, format, depthBuffer, Texture.GenerateAllMips, readWrite)
            { msaaSamples = antiAliasing, enableRandomWrite = enableRandomWrite, memoryless = memorylessMode, useDynamicScale = useDynamicScale }, filter);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing, bool enableRandomWrite, RenderTextureMemoryless memorylessMode) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, readWrite, antiAliasing, enableRandomWrite, memorylessMode, false);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing, bool enableRandomWrite) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, readWrite, antiAliasing, enableRandomWrite, RenderTextureMemoryless.None);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, readWrite, antiAliasing, false);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, RenderTextureFormat format,
        RenderTextureReadWrite readWrite) => GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, readWrite, 1);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, RenderTextureFormat format) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, RenderTextureReadWrite.Default);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, RenderTextureFormat.Default);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, FilterMode.Point);

    public void GetTemporaryRT(int nameID, int width, int height) => GetTemporaryRT(nameID, width, height, 0);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, GraphicsFormat format, int antiAliasing,
        bool enableRandomWrite, RenderTextureMemoryless memorylessMode, bool useDynamicScale) =>
        GetTemporaryRT(nameID, new RenderTextureDescriptor(width, height, format, depthBuffer)
            { msaaSamples = antiAliasing, enableRandomWrite = enableRandomWrite, memoryless = memorylessMode, useDynamicScale = useDynamicScale }, filter);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, GraphicsFormat format, int antiAliasing,
        bool enableRandomWrite, RenderTextureMemoryless memorylessMode) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, antiAliasing, enableRandomWrite, memorylessMode, false);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, GraphicsFormat format, int antiAliasing,
        bool enableRandomWrite) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, antiAliasing, enableRandomWrite, RenderTextureMemoryless.None);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, GraphicsFormat format, int antiAliasing) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, antiAliasing, false);

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, GraphicsFormat format) =>
        GetTemporaryRT(nameID, width, height, depthBuffer, filter, format, 1);

    public void ReleaseTemporaryRT(int nameID)
    {
        ThrowIfReleased();
        Record(() => WorldGraphics.RecordReleaseTemporary(nameID));
    }

    /// <summary>The world's description of a temporary texture. A size of zero or below becomes the screen's size when the buffer runs.</summary>
    private static DisunityRenderTexture Describe(RenderTextureDescriptor desc, FilterMode filter)
    {
        if (desc.width <= 0) desc.width = Math.Max(Screen.width, 1);
        if (desc.height <= 0) desc.height = Math.Max(Screen.height, 1);
        var texture = new RenderTexture(desc) { filterMode = filter, wrapMode = TextureWrapMode.Clamp };
        return texture.Describe();
    }
}
