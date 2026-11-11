namespace UnityEngine;

/// <summary>
/// Graphics.Blit and Graphics.CopyTexture. Each call that the world can hold appends one piece of offscreen work to the
/// frame's list, in call order, and draws nothing itself: the host executes the list with the world's materials and render
/// textures. The destination of a blit or a copy is a <see cref="RenderTexture"/> (created when it is not), and the source is a
/// <see cref="Texture2D"/> with pixels, a render texture, or null for a blit. A blit without a destination draws to the
/// screen, which the engine has no path for yet, and so do the overloads that scale, offset, address a depth slice or a region,
/// BlitMultiTap and copies into a texture that is not a render texture: they throw <see cref="NotSupportedException"/> naming
/// the overload. A copy of a whole texture records one copy per mip level both textures have, on element 0.
/// </summary>
public static partial class Graphics
{
    private static NotSupportedException NoPath(string overload, string reason) =>
        new($"Graphics.{overload} is not supported yet: {reason}");

    public static void Blit(Texture source, RenderTexture dest) => RecordBlit("Blit(Texture, RenderTexture)", source, dest, null, -1);

    public static void Blit(Texture source, RenderTexture dest, Vector2 scale, Vector2 offset) =>
        RecordBlit("Blit(Texture, RenderTexture, Vector2, Vector2)", source, dest, null, -1, scale, offset);

    public static void Blit(Texture source, RenderTexture dest, Vector2 scale, Vector2 offset, int sourceDepthSlice, int destDepthSlice) =>
        RecordBlit("Blit(Texture, RenderTexture, Vector2, Vector2, int, int)", source, dest, null, -1, scale, offset, Math.Max(sourceDepthSlice, destDepthSlice));

    /// <summary>Draws every pass of the material, which is the explicit pass -1. A compiled game binds to this signature itself,
    /// so it is declared apart from the form with a pass; both validate and record through <c>RecordBlit</c>.</summary>
    public static void Blit(Texture source, RenderTexture dest, Material mat) =>
        RecordBlit("Blit(Texture, RenderTexture, Material)", source, dest, mat, -1, mustHaveMaterial: true);

    /// <summary>A null or destroyed material throws, and so does a pass below -1 or, when the shader declares its passes, at or
    /// past the pass count.</summary>
    public static void Blit(Texture source, RenderTexture dest, Material mat, int pass = -1) =>
        RecordBlit("Blit(Texture, RenderTexture, Material, int)", source, dest, mat, pass, mustHaveMaterial: true);

    public static void Blit(Texture source, RenderTexture dest, Material mat, int pass, int destDepthSlice) =>
        RecordBlit("Blit(Texture, RenderTexture, Material, int, int)", source, dest, mat, pass, depthSlice: destDepthSlice, mustHaveMaterial: true);

    /// <summary>Draws into <see cref="RenderTexture.active"/>; with none active the destination is the screen and the call throws.</summary>
    public static void Blit(Texture source, Material mat, int pass = -1) =>
        RecordBlit("Blit(Texture, Material, int)", source, RenderTexture.active, mat, pass, mustHaveMaterial: true);

    public static void Blit(Texture source, Material mat, int pass, int destDepthSlice) =>
        RecordBlit("Blit(Texture, Material, int, int)", source, RenderTexture.active, mat, pass, depthSlice: destDepthSlice, mustHaveMaterial: true);

    public static void BlitMultiTap(Texture source, RenderTexture dest, Material mat, params Vector2[] offsets) =>
        throw NoPath("BlitMultiTap(Texture, RenderTexture, Material, Vector2[])", "the engine records no multi-tap draw");

    public static void BlitMultiTap(Texture source, RenderTexture dest, Material mat, int destDepthSlice, params Vector2[] offsets) =>
        throw NoPath("BlitMultiTap(Texture, RenderTexture, Material, int, Vector2[])", "the engine records no multi-tap draw");

    private static void RecordBlit(string overload, Texture? source, RenderTexture? dest, Material? mat, int pass, Vector2? scale = null,
        Vector2? offset = null, int depthSlice = 0, bool mustHaveMaterial = false)
    {
        if (mustHaveMaterial)
        {
            ArgumentNullException.ThrowIfNull(mat);
            if (!mat) throw new ArgumentException("the material was destroyed", nameof(mat));
            if (pass < -1 || pass >= (mat.DeclaredPassCount ?? int.MaxValue))
                throw new ArgumentException($"pass {pass} is outside the material's passes", nameof(pass));
        }
        if (dest is null) throw NoPath(overload, "a null destination draws to the screen, which the engine has no path for");
        if (!dest) throw new ArgumentException("the destination texture was destroyed", nameof(dest));
        if (scale is { } s && (s != Vector2.one || offset != Vector2.zero)) throw NoPath(overload, "a scale or offset changes what the blit copies, which a record cannot hold");
        if (depthSlice != 0) throw NoPath(overload, "a depth slice other than 0, which a record cannot address");
        var from = WorldGraphics.RequireTexture(source, $"Graphics.{overload}", "source");
        if (!dest.IsCreated()) dest.Create();
        WorldGraphics.RecordBlit(from, dest.WorldId, mat is null ? 0UL : mat.WorldLink.Id, pass);
    }

    /// <summary>Copies every mip level both textures have, on element 0.</summary>
    public static void CopyTexture(Texture src, Texture dst) =>
        CopyMips("CopyTexture(Texture, Texture)", src, 0, dst, 0);

    public static void CopyTexture(Texture src, int srcElement, Texture dst, int dstElement) =>
        CopyMips("CopyTexture(Texture, int, Texture, int)", src, srcElement, dst, dstElement);

    public static void CopyTexture(Texture src, int srcElement, int srcMip, Texture dst, int dstElement, int dstMip)
    {
        var (from, target) = CopyEnds("CopyTexture(Texture, int, int, Texture, int, int)", src, srcElement, dst, dstElement);
        CheckMip(src, srcMip, "srcMip");
        CheckMip(dst, dstMip, "dstMip");
        WorldGraphics.RecordCopy(from, (uint)srcElement, (uint)srcMip, target, (uint)dstElement, (uint)dstMip);
    }

    public static void CopyTexture(Texture src, int srcElement, int srcMip, int srcX, int srcY, int srcWidth, int srcHeight, Texture dst,
        int dstElement, int dstMip, int dstX, int dstY)
    {
        CopyEnds("CopyTexture(Texture, int, int, int, int, int, int, Texture, int, int, int, int)", src, srcElement, dst, dstElement);
        throw NoPath("CopyTexture(Texture, int, int, int, int, int, int, Texture, int, int, int, int)", "a copy of a region is not recorded");
    }

    private static void CopyMips(string overload, Texture src, int srcElement, Texture dst, int dstElement)
    {
        var (from, target) = CopyEnds(overload, src, srcElement, dst, dstElement);
        for (var mip = 0; mip < Math.Min(src.mipmapCount, dst.mipmapCount); mip++)
            WorldGraphics.RecordCopy(from, (uint)srcElement, (uint)mip, target, (uint)dstElement, (uint)mip);
    }

    /// <summary>Validates the two textures of a copy and returns the source as the world holds it and the destination's render
    /// texture id.</summary>
    private static ((uint Kind, ulong Texture) Source, ulong Target) CopyEnds(string overload, Texture src, int srcElement, Texture dst, int dstElement)
    {
        ArgumentNullException.ThrowIfNull(src);
        ArgumentNullException.ThrowIfNull(dst);
        if (!src) throw new ArgumentException("the source texture was destroyed", nameof(src));
        if (!dst) throw new ArgumentException("the destination texture was destroyed", nameof(dst));
        if (srcElement < 0) throw new ArgumentException($"element {srcElement} is negative", nameof(srcElement));
        if (dstElement < 0) throw new ArgumentException($"element {dstElement} is negative", nameof(dstElement));
        if (dst is not RenderTexture target) throw NoPath(overload, $"a {dst.GetType().Name} destination is not a render texture");
        var from = WorldGraphics.RequireTexture(src, $"Graphics.{overload}", "source");
        if (!target.IsCreated()) target.Create();
        return (from, target.WorldId);
    }

    private static void CheckMip(Texture texture, int mip, string name)
    {
        if (mip < 0 || mip >= texture.mipmapCount)
            throw new ArgumentException($"mip level {mip} is outside 0 to {texture.mipmapCount - 1} of texture '{texture.name}'", name);
    }
}
