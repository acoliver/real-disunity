using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// Render passes recorded on a command buffer, as a render graph's native pass compiler records them. The engine has no tile
/// memory, so a pass becomes the target bindings and clears it implies: at <see cref="BeginRenderPass(int, int, int, int,
/// NativeArray{AttachmentDescriptor}, int, NativeArray{SubPassDescriptor}, ReadOnlySpan{byte})"/> each attachment whose load action
/// is Clear is bound alone and cleared to its clear values, then the first sub-pass's color outputs are bound with the depth
/// attachment; <see cref="NextSubPass"/> binds the next sub-pass's color outputs with the same depth; <see cref="EndRenderPass"/>
/// records nothing. Store actions, resolve targets, sample counts, sub-pass inputs and sub-pass flags have no effect, which is the
/// engine's choice. The bindings are recorded even while <see cref="ThrowOnSetRenderTarget"/> is set, because the pass, not a
/// script, binds them.
/// </summary>
public sealed partial class CommandBuffer
{
    private sealed class OpenPass(RenderTargetIdentifier[] targets, RenderTargetIdentifier? depth, AttachmentIndexArray[] outputs)
    {
        public readonly RenderTargetIdentifier[] Targets = targets;
        public readonly RenderTargetIdentifier? Depth = depth;
        public readonly AttachmentIndexArray[] Outputs = outputs;
        public int SubPass;
    }

    private OpenPass? _pass;

    public void BeginRenderPass(int width, int height, int volumeDepth, int samples, NativeArray<AttachmentDescriptor> attachments,
        int depthAttachmentIndex, NativeArray<SubPassDescriptor> subPasses, ReadOnlySpan<byte> debugNameUtf8) =>
        BeginPass(attachments.ToArray(), depthAttachmentIndex, subPasses.ToArray());

    /// <summary>The form that also names a shading-rate image attachment. The engine has no shading-rate images, so the only
    /// index it takes is -1 (none), which makes this the form without it; any other index throws
    /// <see cref="NotSupportedException"/> naming the parameter and records nothing. The parameter list is the one built
    /// players call, which is why it is longer than the engine's own methods.</summary>
    public void BeginRenderPass(int width, int height, int volumeDepth, int samples, NativeArray<AttachmentDescriptor> attachments,
        int depthAttachmentIndex, int shadingRateImageAttachmentIndex, NativeArray<SubPassDescriptor> subPasses,
        ReadOnlySpan<byte> debugNameUtf8)
    {
        ThrowIfReleased();
        if (shadingRateImageAttachmentIndex != -1)
            throw new NotSupportedException(
                $"CommandBuffer.BeginRenderPass: {nameof(shadingRateImageAttachmentIndex)} {shadingRateImageAttachmentIndex} names a shading-rate image, which the engine does not have; only -1 (none) is supported");
        BeginPass(attachments.ToArray(), depthAttachmentIndex, subPasses.ToArray());
    }

    public void BeginRenderPass(int width, int height, int volumeDepth, int samples, NativeArray<AttachmentDescriptor> attachments,
        int depthAttachmentIndex, NativeArray<SubPassDescriptor> subPasses) =>
        BeginPass(attachments.ToArray(), depthAttachmentIndex, subPasses.ToArray());

    public void BeginRenderPass(int width, int height, int samples, NativeArray<AttachmentDescriptor> attachments,
        int depthAttachmentIndex, NativeArray<SubPassDescriptor> subPasses) =>
        BeginPass(attachments.ToArray(), depthAttachmentIndex, subPasses.ToArray());

    /// <summary>Binds the color outputs of the next sub-pass with the pass's depth attachment.</summary>
    public void NextSubPass()
    {
        ThrowIfReleased();
        var pass = _pass ?? throw new InvalidOperationException("CommandBuffer.NextSubPass: no render pass is open");
        if (pass.SubPass + 1 >= pass.Outputs.Length)
            throw new InvalidOperationException($"CommandBuffer.NextSubPass: the render pass has {pass.Outputs.Length} sub-passes");
        pass.SubPass++;
        BindSubPass(pass);
    }

    /// <summary>Closes the open render pass; records nothing.</summary>
    public void EndRenderPass()
    {
        ThrowIfReleased();
        if (_pass is null) throw new InvalidOperationException("CommandBuffer.EndRenderPass: no render pass is open");
        _pass = null;
    }

    private void BeginPass(AttachmentDescriptor[] attachments, int depthIndex, SubPassDescriptor[] subPasses)
    {
        ThrowIfReleased();
        const string member = "CommandBuffer.BeginRenderPass";
        if (_pass is not null) throw new InvalidOperationException($"{member}: a render pass is already open");
        if (depthIndex < -1 || depthIndex >= attachments.Length)
            throw new ArgumentOutOfRangeException(nameof(depthIndex), depthIndex, $"{member}: the depth attachment index is outside the {attachments.Length} attachments");
        if (subPasses.Length == 0) throw new ArgumentException($"{member}: a render pass needs at least one sub-pass", nameof(subPasses));
        var outputs = subPasses.Select(sub => sub.colorOutputs).ToArray();
        foreach (var list in outputs)
            for (var i = 0; i < list.Length; i++)
                if (list[i] < 0 || list[i] >= attachments.Length || list[i] == depthIndex)
                    throw new ArgumentException($"{member}: a sub-pass names attachment {list[i]}, which is not a color attachment of the pass", nameof(subPasses));
        var targets = attachments.Select(attachment => attachment.loadStoreTarget).ToArray();
        var depth = depthIndex >= 0 ? targets[depthIndex] : (RenderTargetIdentifier?)null;
        ClearOnLoad(attachments, depthIndex);
        _pass = new OpenPass(targets, depth, outputs);
        BindSubPass(_pass);
    }

    /// <summary>Clears each attachment whose load action is Clear, bound alone: a color attachment to its clear color, the
    /// depth attachment to its clear depth and stencil.</summary>
    private void ClearOnLoad(AttachmentDescriptor[] attachments, int depthIndex)
    {
        for (var index = 0; index < attachments.Length; index++)
        {
            var attachment = attachments[index];
            if (attachment.loadAction != RenderBufferLoadAction.Clear) continue;
            if (index == depthIndex)
            {
                BindTargets("BeginRenderPass", [], attachment.loadStoreTarget, null, null, null);
                RecordClear(2u | 4u, default, attachment.clearDepth, attachment.clearStencil);
            }
            else
            {
                BindTargets("BeginRenderPass", [attachment.loadStoreTarget], null, null, null, null);
                RecordClear(1u, attachment.clearColor, 1f, 0);
            }
        }
    }

    private void BindSubPass(OpenPass pass)
    {
        var list = pass.Outputs[pass.SubPass];
        var colors = Enumerable.Range(0, list.Length).Select(i => pass.Targets[list[i]]).ToArray();
        BindTargets(pass.SubPass == 0 ? "BeginRenderPass" : "NextSubPass", colors, pass.Depth, null, null, null);
    }
}
