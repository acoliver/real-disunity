using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// Temporary render textures. GetTemporary hands out a released texture with an identical descriptor when
/// there is one and creates a texture otherwise; ReleaseTemporary puts a texture back. The pool keeps the
/// 64 most recently released textures and destroys older ones.
/// </summary>
public partial class RenderTexture
{
    private const int PoolLimit = 64;

    private static readonly List<RenderTexture> Pool = new();
    private static readonly HashSet<RenderTexture> Leased = new(ReferenceEqualityComparer.Instance);
    private static int _temporaryCount;

    public static RenderTexture GetTemporary(RenderTextureDescriptor desc)
    {
        Validate(desc);
        for (var i = 0; i < Pool.Count; i++)
        {
            if (!Pool[i] || !RenderTextureDescriptor.Same(Pool[i]._desc, desc)) continue;
            var reused = Pool[i];
            Pool.RemoveAt(i);
            Leased.Add(reused);
            return reused;
        }
        var texture = new RenderTexture(desc)
        {
            name = $"TempBuffer {Interlocked.Increment(ref _temporaryCount)} {desc.width} x {desc.height}",
            hideFlags = HideFlags.HideAndDontSave,
        };
        texture.Create();
        Leased.Add(texture);
        return texture;
    }

    /// <summary>Throws <see cref="ArgumentException"/> for a texture GetTemporary did not hand out or one already released.</summary>
    public static void ReleaseTemporary(RenderTexture temp)
    {
        ArgumentNullException.ThrowIfNull(temp);
        if (!Leased.Remove(temp))
            throw new ArgumentException("The texture was not returned by GetTemporary or is already released", nameof(temp));
        if (ReferenceEquals(_active, temp)) _active = null;
        Pool.Add(temp);
        while (Pool.Count > PoolLimit)
        {
            Pool[0].MarkAssetDestroyed();
            Pool.RemoveAt(0);
        }
    }

    private static RenderTextureDescriptor Temporary(int width, int height, int depthBuffer, GraphicsFormat format, int antiAliasing,
        RenderTextureMemoryless memorylessMode, VRTextureUsage vrUsage, bool useDynamicScale)
    {
        var desc = new RenderTextureDescriptor(width, height, format, depthBuffer)
        {
            msaaSamples = antiAliasing, memoryless = memorylessMode, vrUsage = vrUsage, useDynamicScale = useDynamicScale,
        };
        return desc;
    }

    private static RenderTextureDescriptor Temporary(int width, int height, int depthBuffer, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing, RenderTextureMemoryless memorylessMode, VRTextureUsage vrUsage, bool useDynamicScale)
    {
        var desc = new RenderTextureDescriptor(width, height, format, depthBuffer, Texture.GenerateAllMips, readWrite)
        {
            msaaSamples = antiAliasing, memoryless = memorylessMode, vrUsage = vrUsage, useDynamicScale = useDynamicScale,
        };
        return desc;
    }

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, GraphicsFormat format, int antiAliasing,
        RenderTextureMemoryless memorylessMode, VRTextureUsage vrUsage, bool useDynamicScale) =>
        GetTemporary(Temporary(width, height, depthBuffer, format, antiAliasing, memorylessMode, vrUsage, useDynamicScale));

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, GraphicsFormat format, int antiAliasing,
        RenderTextureMemoryless memorylessMode, VRTextureUsage vrUsage) =>
        GetTemporary(width, height, depthBuffer, format, antiAliasing, memorylessMode, vrUsage, false);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, GraphicsFormat format, int antiAliasing,
        RenderTextureMemoryless memorylessMode) =>
        GetTemporary(width, height, depthBuffer, format, antiAliasing, memorylessMode, VRTextureUsage.None);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, GraphicsFormat format, int antiAliasing) =>
        GetTemporary(width, height, depthBuffer, format, antiAliasing, RenderTextureMemoryless.None);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, GraphicsFormat format) =>
        GetTemporary(width, height, depthBuffer, format, 1);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing, RenderTextureMemoryless memorylessMode, VRTextureUsage vrUsage, bool useDynamicScale) =>
        GetTemporary(Temporary(width, height, depthBuffer, format, readWrite, antiAliasing, memorylessMode, vrUsage, useDynamicScale));

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing, RenderTextureMemoryless memorylessMode, VRTextureUsage vrUsage) =>
        GetTemporary(width, height, depthBuffer, format, readWrite, antiAliasing, memorylessMode, vrUsage, false);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing, RenderTextureMemoryless memorylessMode) =>
        GetTemporary(width, height, depthBuffer, format, readWrite, antiAliasing, memorylessMode, VRTextureUsage.None);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, RenderTextureFormat format,
        RenderTextureReadWrite readWrite, int antiAliasing) =>
        GetTemporary(width, height, depthBuffer, format, readWrite, antiAliasing, RenderTextureMemoryless.None);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, RenderTextureFormat format, RenderTextureReadWrite readWrite) =>
        GetTemporary(width, height, depthBuffer, format, readWrite, 1);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer, RenderTextureFormat format) =>
        GetTemporary(width, height, depthBuffer, format, RenderTextureReadWrite.Default);

    public static RenderTexture GetTemporary(int width, int height, int depthBuffer) =>
        GetTemporary(width, height, depthBuffer, RenderTextureFormat.Default);

    public static RenderTexture GetTemporary(int width, int height) => GetTemporary(width, height, 0);
}
