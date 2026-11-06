using Disunity.Native;

namespace UnityEngine.Rendering;

/// <summary>
/// A <see cref="RenderTargetIdentifier"/> as a command buffer holds it: what it names, checked when the command is recorded, and
/// the render texture it names kept as an object, so the world resource is made (as <see cref="Graphics.Blit(Texture, RenderTexture)"/> does)
/// only when the command is executed. A <see cref="RenderTexture"/> names itself, an integer or string names a temporary render
/// texture by its property name id, <see cref="BuiltinRenderTextureType.CameraTarget"/> names the target of the camera being rendered and
/// <see cref="BuiltinRenderTextureType.None"/> names nothing. Every other built-in target (the camera's depth, the G-buffer, motion
/// vectors and the rest), a texture that is not a render texture and a render buffer have no world counterpart, so recording one throws
/// <see cref="NotSupportedException"/> naming it.
/// </summary>
internal readonly record struct CommandTarget(uint Kind, int NameId, RenderTexture? Texture, int Mip, int Face, int Slice)
{
    private const uint KindRender = 0, KindTemporary = 1, KindCamera = 2, KindNone = 3;

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
            default:
                throw new NotSupportedException($"{member}: the render target {Describe(id)} is not supported yet");
        }
    }

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
        if (Texture is null || !Texture) throw new InvalidOperationException("a render target was destroyed after the command was recorded");
        if (!Texture.IsCreated()) Texture.Create();
        return new DisunityTargetRef { kind = KindRender, render = Texture.WorldId };
    }

    /// <summary>Returns this target when a blit, copy or global texture can name it: only a render texture or nothing has a world
    /// form as a texture, and any other target throws <see cref="NotSupportedException"/> naming <paramref name="member"/>.</summary>
    internal CommandTarget AsTexture(string member) => Kind is KindRender or KindNone
        ? this
        : throw new NotSupportedException($"{member}: a {(Kind == KindTemporary ? "temporary render texture" : "camera target")} as a texture is not supported yet");

    /// <summary>The texture kind and identity the world records: 0 for none and 2 for a render texture.</summary>
    internal (uint Kind, ulong Texture) ToTexture() => Kind == KindNone ? (0u, 0UL) : (2u, ToNative().render);
}
