using Disunity.Native;

namespace UnityEngine.Rendering;

/// <summary>
/// A <see cref="RenderTargetIdentifier"/> as a command buffer holds it: what it names, checked when the command is recorded, and
/// the render texture it names kept as an object, so the world resource is made (as <see cref="Graphics.Blit(Texture, RenderTexture)"/> does)
/// only when the command is executed. A <see cref="RenderTexture"/> names itself, an integer or string names a temporary render
/// texture by its property name id, <see cref="BuiltinRenderTextureType.CameraTarget"/> names the target of the camera being rendered and
/// <see cref="BuiltinRenderTextureType.None"/> names nothing. A <see cref="Texture2D"/> names itself as a texture to read, and only a
/// texture is read from it: as a render target it throws <see cref="NotSupportedException"/>. Every other built-in target (the camera's
/// depth, the G-buffer, motion vectors and the rest), any other texture and a render buffer have no world counterpart, so recording one
/// throws <see cref="NotSupportedException"/> naming it.
/// </summary>
internal readonly record struct CommandTarget(uint Kind, int NameId, Texture? Texture, int Mip, int Face, int Slice)
{
    private const uint KindRender = 0, KindTemporary = 1, KindCamera = 2, KindNone = 3, KindAsset = 4;

    internal static CommandTarget Of(in RenderTargetIdentifier id, string member)
    {
        var (mip, face, slice) = (id.m_MipLevel, (int)id.m_CubeFace, id.m_DepthSlice);
        switch (id.m_Type)
        {
            case BuiltinRenderTextureType.None:
                return new(KindNone, 0, null, 0, -1, -1);
            case BuiltinRenderTextureType.CameraTarget:
                return new(KindCamera, 0, null, mip, face, slice);
            case BuiltinRenderTextureType.PropertyName:
                return new(KindTemporary, id.m_NameID, null, mip, face, slice);
            case BuiltinRenderTextureType.RenderTexture:
                var texture = ObjectRegistry.Find(id.m_InstanceID) as RenderTexture
                    ?? throw new ArgumentException($"{member}: the render texture of the target was destroyed");
                return new(KindRender, 0, texture, mip, face, slice);
            case BuiltinRenderTextureType.BindableTexture:
                return OfTexture(id, member);
            default:
                throw new NotSupportedException($"{member}: the render target {Describe(id)} is not supported yet");
        }
    }

    /// <summary>A texture that is not a render texture: a live <see cref="Texture2D"/> is read as the asset the world holds; a
    /// destroyed one throws <see cref="ArgumentException"/> and any other kind of texture <see cref="NotSupportedException"/>.</summary>
    private static CommandTarget OfTexture(in RenderTargetIdentifier id, string member) => ObjectRegistry.Find(id.m_InstanceID) switch
    {
        Texture2D { width: > 0, height: > 0 } texture when texture => new(KindAsset, 0, texture, id.m_MipLevel, (int)id.m_CubeFace, id.m_DepthSlice),
        Texture other when other => throw new NotSupportedException($"{member}: a {other.GetType().Name} as a command buffer texture is not supported yet"),
        _ => throw new ArgumentException($"{member}: the texture of the target was destroyed"),
    };

    /// <summary>Returns this target when draws can go to it: a texture asset is only read, so it throws
    /// <see cref="NotSupportedException"/> naming <paramref name="member"/>.</summary>
    internal CommandTarget AsTarget(string member) => Kind == KindAsset
        ? throw new NotSupportedException($"{member}: a {Texture!.GetType().Name} as a render target is not supported; only a render texture is drawn into")
        : this;

    /// <summary>A depth target: <see cref="BuiltinRenderTextureType.Depth"/> is the depth buffer of the camera being rendered, so it
    /// names the camera target, whose depth buffer the host binds; any other identifier is resolved as <see cref="Of"/> does.</summary>
    internal static CommandTarget OfDepth(in RenderTargetIdentifier id, string member) => id.m_Type == BuiltinRenderTextureType.Depth
        ? new(KindCamera, 0, null, id.m_MipLevel, (int)id.m_CubeFace, id.m_DepthSlice)
        : Of(id, member);

    /// <summary>The target of a camera: its target texture, or the camera target when it draws to the screen.</summary>
    internal static CommandTarget OfCamera(RenderTexture? texture) =>
        texture is null ? new(KindCamera, 0, null, 0, -1, -1) : new(KindRender, 0, texture, 0, -1, -1);

    private static string Describe(in RenderTargetIdentifier id) => id.m_Type switch
    {
        BuiltinRenderTextureType.BindableTexture => "BuiltinRenderTextureType.BindableTexture (a texture that is not a render texture)",
        BuiltinRenderTextureType.BufferPtr => "BuiltinRenderTextureType.BufferPtr (a render buffer)",
        var other => $"BuiltinRenderTextureType.{other}",
    };

    /// <summary>The target as the world holds it. A render texture that was destroyed since the command was recorded throws
    /// <see cref="InvalidOperationException"/>.</summary>
    internal DisunityTargetRef ToNative()
    {
        if (Kind != KindRender) return new DisunityTargetRef { kind = Kind, name_id = Kind == KindTemporary ? NameId : 0 };
        if (Texture is not RenderTexture texture || !texture) throw new InvalidOperationException("a render target was destroyed after the command was recorded");
        if (!texture.IsCreated()) texture.Create();
        return new DisunityTargetRef { kind = KindRender, render = texture.WorldId };
    }

    /// <summary>Returns this target when a blit, copy or global texture can name it: a render texture, a texture asset or nothing
    /// has a world form as a texture, and any other target throws <see cref="NotSupportedException"/> naming <paramref name="member"/>.</summary>
    internal CommandTarget AsTexture(string member) => Kind is KindRender or KindNone or KindAsset
        ? this
        : throw new NotSupportedException($"{member}: a {(Kind == KindTemporary ? "temporary render texture" : "camera target")} as a texture is not supported yet");

    /// <summary>The texture kind and identity the world records: none, a texture asset by its renderer identity, or a render
    /// texture. A texture destroyed since the command was recorded throws <see cref="InvalidOperationException"/>.</summary>
    internal (uint Kind, ulong Texture) ToTexture() => Kind switch
    {
        KindNone => (0u, 0UL),
        KindAsset => Texture is { } texture && texture
            ? WorldGraphics.TextureOf(texture)
            : throw new InvalidOperationException("a texture was destroyed after the command was recorded"),
        _ => (2u, ToNative().render),
    };
}
