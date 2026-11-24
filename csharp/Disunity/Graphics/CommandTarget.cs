using Disunity.Native;

namespace UnityEngine.Rendering;

/// <summary>
/// A <see cref="RenderTargetIdentifier"/> as a command buffer holds it: what it names, checked when the command is recorded, and
/// the render texture it names kept as an object, so the world resource is made (as <see cref="Graphics.Blit(Texture, RenderTexture)"/> does)
/// only when the command is executed. A <see cref="RenderTexture"/> names itself, an integer or string names a temporary render
/// texture by its property name id, <see cref="BuiltinRenderTextureType.CameraTarget"/> names the target of the camera being rendered and
/// <see cref="BuiltinRenderTextureType.None"/> names nothing. A texture that is not a render texture (a <see cref="Texture2D"/>,
/// <see cref="Texture2DArray"/>, <see cref="Cubemap"/> or <see cref="Texture3D"/>) names itself as a texture to read, under the rule of
/// <see cref="Shader.SetGlobalTexture(string, Texture)"/>: <see cref="ToTexture"/> sends it as <see cref="WorldGraphics.TextureOf(Texture?)"/>
/// does, as the asset the host draws by, or as named and not sent when the world cannot hold it. Only a texture is read from it: as a
/// render target it throws <see cref="NotSupportedException"/>. The host reads any stored texture as the source of a blit or a
/// copy, a layer of an array, cube or volume by the element a copy names and the first layer for a blit without a material, so a
/// source is as free as a global; <see cref="ToSource"/> refuses only a texture the world cannot send. Every other built-in target (the
/// camera's depth, the G-buffer, motion vectors and the rest) and a render buffer have no world counterpart, so recording one throws
/// <see cref="NotSupportedException"/> naming it.
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

    /// <summary>A texture that is not a render texture: a live texture is read as the asset the world holds, whatever its kind; a
    /// destroyed one throws <see cref="ArgumentException"/>.</summary>
    private static CommandTarget OfTexture(in RenderTargetIdentifier id, string member) => ObjectRegistry.Find(id.m_InstanceID) switch
    {
        Texture texture when texture => new(KindAsset, 0, texture, id.m_MipLevel, (int)id.m_CubeFace, id.m_DepthSlice),
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

    /// <summary>Returns this target when a blit, copy or global texture can name it: a render texture, a temporary render texture,
    /// a texture asset of any kind or nothing has a world form as a texture, and the camera target throws <see cref="NotSupportedException"/>
    /// naming <paramref name="member"/>.</summary>
    internal CommandTarget AsTexture(string member) => Kind is KindRender or KindTemporary or KindNone or KindAsset
        ? this
        : throw new NotSupportedException($"{member}: a camera target as a texture is not supported yet");

    /// <summary>Returns this target when a blit or a copy can write it: a render texture or a temporary render texture. Any other
    /// target throws <see cref="NotSupportedException"/> naming <paramref name="member"/>.</summary>
    internal CommandTarget AsWorkTarget(string member) => Kind is KindRender or KindTemporary
        ? this
        : throw new NotSupportedException($"{member}: the destination must be a render texture or a temporary render texture");

    /// <summary>The target kind and identity a blit or a copy records: a temporary render texture by its property name id, or a
    /// render texture by its world id. A render texture destroyed since the command was recorded throws
    /// <see cref="InvalidOperationException"/>.</summary>
    internal (uint Kind, ulong Target) ToWorkTarget() => Kind == KindTemporary
        ? (WorldGraphics.TargetTemporary, (uint)NameId)
        : (WorldGraphics.TargetRender, ToNative().render);

    /// <summary>The texture kind and identity the world records: none, a texture asset by the identity the host draws it by, a render
    /// texture, or a temporary render texture by its property name id. A texture destroyed since the command was recorded throws
    /// <see cref="InvalidOperationException"/>.</summary>
    internal (uint Kind, ulong Texture) ToTexture() => Kind switch
    {
        KindNone => (0u, 0UL),
        KindTemporary => (WorldGraphics.TextureTemporary, (uint)NameId),
        KindAsset => Texture is { } texture && texture
            ? WorldGraphics.TextureOf(texture)
            : throw new InvalidOperationException("a texture was destroyed after the command was recorded"),
        _ => (2u, ToNative().render),
    };

    /// <summary>The texture a blit or a copy reads, as <see cref="ToTexture"/> sends it, with the rule of
    /// <see cref="WorldGraphics.RequireTexture"/> that <c>Blit(Texture, ...)</c> follows: a texture the world cannot send (an empty
    /// <see cref="Texture2D"/>, or an array, cube or volume the host cannot store) throws <see cref="NotSupportedException"/>
    /// naming <paramref name="member"/>, because the host would draw nothing from it.</summary>
    internal (uint Kind, ulong Texture) ToSource(string member)
    {
        var source = ToTexture();
        return Kind == KindAsset && WorldGraphics.IsUnbound(source.Kind)
            ? throw new NotSupportedException($"{member}: a {Texture!.GetType().Name} source is not supported yet")
            : source;
    }
}
