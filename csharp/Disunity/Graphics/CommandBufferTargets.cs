using Disunity.Native;

namespace UnityEngine.Rendering;

/// <summary>
/// The recording members that bind and clear render targets, blit and copy. A render target is a
/// <see cref="RenderTargetIdentifier"/> (see <see cref="CommandTarget"/>); the mip level, cube face and depth slice come from the
/// identifier unless the overload gives them, and an overload that gives only some of them leaves the face unset (all faces) and the
/// slice 0. The load and store actions of the overloads that take them are accepted and have no effect, which is the engine's
/// choice: it has no tile memory whose contents they would keep or drop. A clear that names individual color attachments
/// (<see cref="RTClearFlags.Color0"/> to <see cref="RTClearFlags.Color7"/>) clears the color.
/// </summary>
public sealed partial class CommandBuffer
{
    /// <summary>While true, every SetRenderTarget overload throws <see cref="InvalidOperationException"/> instead of recording.
    /// A render graph sets it around the passes it runs, which bind their targets through the graph. The engine declares it
    /// public so the pipeline packages that set it reach it through the module facade.</summary>
    public static bool ThrowOnSetRenderTarget;

    public void SetRenderTarget(RenderTargetIdentifier rt) => SetTargets(nameof(SetRenderTarget), [rt], null, null, null, null);

    public void SetRenderTarget(RenderTargetIdentifier rt, int mipLevel) =>
        SetTargets(nameof(SetRenderTarget), [rt], null, mipLevel, CubemapFace.Unknown, 0);

    public void SetRenderTarget(RenderTargetIdentifier rt, int mipLevel, CubemapFace cubemapFace) =>
        SetTargets(nameof(SetRenderTarget), [rt], null, mipLevel, cubemapFace, 0);

    public void SetRenderTarget(RenderTargetIdentifier rt, int mipLevel, CubemapFace cubemapFace, int depthSlice) =>
        SetTargets(nameof(SetRenderTarget), [rt], null, mipLevel, cubemapFace, depthSlice);

    public void SetRenderTarget(RenderTargetIdentifier rt, RenderBufferLoadAction loadAction, RenderBufferStoreAction storeAction) =>
        SetTargets(nameof(SetRenderTarget), [rt], null, null, null, null);

    public void SetRenderTarget(RenderTargetIdentifier rt, RenderBufferLoadAction colorLoadAction, RenderBufferStoreAction colorStoreAction,
        RenderBufferLoadAction depthLoadAction, RenderBufferStoreAction depthStoreAction) =>
        SetTargets(nameof(SetRenderTarget), [rt], null, null, null, null);

    public void SetRenderTarget(RenderTargetIdentifier color, RenderTargetIdentifier depth) =>
        SetTargets(nameof(SetRenderTarget), [color], depth, null, null, null);

    public void SetRenderTarget(RenderTargetIdentifier color, RenderTargetIdentifier depth, int mipLevel) =>
        SetTargets(nameof(SetRenderTarget), [color], depth, mipLevel, CubemapFace.Unknown, 0);

    public void SetRenderTarget(RenderTargetIdentifier color, RenderTargetIdentifier depth, int mipLevel, CubemapFace cubemapFace) =>
        SetTargets(nameof(SetRenderTarget), [color], depth, mipLevel, cubemapFace, 0);

    public void SetRenderTarget(RenderTargetIdentifier color, RenderTargetIdentifier depth, int mipLevel, CubemapFace cubemapFace, int depthSlice) =>
        SetTargets(nameof(SetRenderTarget), [color], depth, mipLevel, cubemapFace, depthSlice);

    public void SetRenderTarget(RenderTargetIdentifier color, RenderBufferLoadAction colorLoadAction, RenderBufferStoreAction colorStoreAction,
        RenderTargetIdentifier depth, RenderBufferLoadAction depthLoadAction, RenderBufferStoreAction depthStoreAction) =>
        SetTargets(nameof(SetRenderTarget), [color], depth, null, null, null);

    public void SetRenderTarget(RenderTargetIdentifier[] colors, RenderTargetIdentifier depth) =>
        SetTargets(nameof(SetRenderTarget), colors, depth, null, null, null);

    public void SetRenderTarget(RenderTargetIdentifier[] colors, RenderTargetIdentifier depth, int mipLevel, CubemapFace cubemapFace, int depthSlice) =>
        SetTargets(nameof(SetRenderTarget), colors, depth, mipLevel, cubemapFace, depthSlice);

    private void SetTargets(string member, RenderTargetIdentifier[] colors, RenderTargetIdentifier? depth, int? mip, CubemapFace? face, int? slice)
    {
        ArgumentNullException.ThrowIfNull(colors);
        ThrowIfReleased();
        if (ThrowOnSetRenderTarget)
            throw new InvalidOperationException($"CommandBuffer.{member} is not allowed while ThrowOnSetRenderTarget is set; a render graph pass binds its targets through the graph");
        BindTargets(member, colors, depth, mip, face, slice);
    }

    /// <summary>Records a binding of the targets, whatever <see cref="ThrowOnSetRenderTarget"/> says.</summary>
    private void BindTargets(string member, RenderTargetIdentifier[] colors, RenderTargetIdentifier? depth, int? mip, CubemapFace? face, int? slice)
    {
        var member2 = $"CommandBuffer.{member}";
        var targets = colors.Select(color => CommandTarget.Of(color, member2)).ToArray();
        var depthTarget = depth is { } d ? CommandTarget.Of(d, member2) : (CommandTarget?)null;
        var first = targets.Length > 0 ? targets[0] : depthTarget ?? default;
        var (useMip, useFace, useSlice) = (mip ?? first.Mip, face is { } f ? (int)f : first.Face, slice ?? first.Slice);
        if (useMip < 0) throw new ArgumentOutOfRangeException(nameof(mip), useMip, $"{member2}: the mip level is negative");
        Record(() => WorldGraphics.RecordSetRenderTarget(targets.Select(target => target.ToNative()).ToArray(), depthTarget?.ToNative(),
            (uint)useMip, useFace, useSlice));
    }

    public void ClearRenderTarget(bool clearDepth, bool clearColor, Color backgroundColor) =>
        ClearRenderTarget(clearDepth, clearColor, backgroundColor, 1f);

    public void ClearRenderTarget(bool clearDepth, bool clearColor, Color backgroundColor, float depth) =>
        RecordClear((clearColor ? 1u : 0u) | (clearDepth ? 2u : 0u), backgroundColor, depth, 0);

    public void ClearRenderTarget(RTClearFlags clearFlags, Color backgroundColor, float depth, uint stencil)
    {
        var colors = RTClearFlags.Color | RTClearFlags.Color0 | RTClearFlags.Color1 | RTClearFlags.Color2 | RTClearFlags.Color3
            | RTClearFlags.Color4 | RTClearFlags.Color5 | RTClearFlags.Color6 | RTClearFlags.Color7;
        var flags = ((clearFlags & colors) != 0 ? 1u : 0u) | ((clearFlags & RTClearFlags.Depth) != 0 ? 2u : 0u)
            | ((clearFlags & RTClearFlags.Stencil) != 0 ? 4u : 0u);
        RecordClear(flags, backgroundColor, depth, stencil);
    }

    private void RecordClear(uint flags, Color color, float depth, uint stencil) =>
        Record(() => WorldGraphics.RecordClear(flags, color, depth, stencil));

    public void Blit(Texture source, RenderTargetIdentifier dest) => BlitTexture(source, dest, null, -1);
    public void Blit(Texture source, RenderTargetIdentifier dest, Material mat) => BlitTexture(source, dest, mat, -1);
    public void Blit(Texture source, RenderTargetIdentifier dest, Material mat, int pass) => BlitTexture(source, dest, mat, pass);
    public void Blit(RenderTargetIdentifier source, RenderTargetIdentifier dest) => BlitTarget(source, dest, null, -1);
    public void Blit(RenderTargetIdentifier source, RenderTargetIdentifier dest, Material mat) => BlitTarget(source, dest, mat, -1);
    public void Blit(RenderTargetIdentifier source, RenderTargetIdentifier dest, Material mat, int pass) => BlitTarget(source, dest, mat, pass);

    private const string BlitMember = "CommandBuffer.Blit";

    /// <summary>A texture source is a <see cref="Texture2D"/> with pixels, a render texture or null, as for <see cref="Graphics.Blit(Texture, RenderTexture)"/>.</summary>
    private void BlitTexture(Texture? source, RenderTargetIdentifier dest, Material? mat, int pass) =>
        RecordBlit(() => WorldGraphics.RequireTexture(source, BlitMember, "source"), dest, mat, pass);

    private void BlitTarget(RenderTargetIdentifier source, RenderTargetIdentifier dest, Material? mat, int pass)
    {
        var from = CommandTarget.Of(source, BlitMember).AsTexture(BlitMember);
        RecordBlit(from.ToTexture, dest, mat, pass);
    }

    private void RecordBlit(Func<(uint Kind, ulong Texture)> source, RenderTargetIdentifier dest, Material? mat, int pass)
    {
        ThrowIfReleased();
        if (pass < -1) throw new ArgumentException($"pass {pass} is below -1", nameof(pass));
        var to = CommandTarget.Of(dest, BlitMember);
        if (to.Kind != 0) throw new NotSupportedException($"{BlitMember}: the destination must be a render texture");
        Record(() => WorldGraphics.RecordBlit(source(), to.ToNative().render, mat is null || !mat ? 0UL : mat.WorldLink.Id, pass));
    }

    public void CopyTexture(RenderTargetIdentifier src, int srcElement, int srcMip, RenderTargetIdentifier dst, int dstElement, int dstMip)
    {
        ThrowIfReleased();
        if (srcElement < 0 || srcMip < 0 || dstElement < 0 || dstMip < 0)
            throw new ArgumentException("an element or mip level of a copy is negative");
        var full = "CommandBuffer.CopyTexture";
        var from = CommandTarget.Of(src, full).AsTexture(full);
        var to = CommandTarget.Of(dst, full);
        if (to.Kind != 0) throw new NotSupportedException($"{full}: the destination must be a render texture");
        Record(() => WorldGraphics.RecordCopy(from.ToTexture(), (uint)srcElement, (uint)srcMip, to.ToNative().render, (uint)dstElement, (uint)dstMip));
    }
}
