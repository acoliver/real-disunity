using Disunity.Hosting;
using Unity.Collections;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>Render passes on a command buffer become the target bindings and clears they imply, compared with the same
/// bindings and clears recorded through SetRenderTarget and ClearRenderTarget.</summary>
internal static class CommandBufferRenderPassTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            PassesBindAndClearAsTheirAttachmentsSay();
            Console.WriteLine("PASS: a render pass clears its Clear attachments, binds each sub-pass's color outputs with the depth and records nothing at its end");
            MisusedPassesAreRefused();
            Console.WriteLine("PASS: render passes refuse nesting, a missing pass, a sub-pass past the last and attachments the pass does not have");
            TheShadingRateFormWithNoImageIsTheFormWithout();
            Console.WriteLine("PASS: BeginRenderPass with shading-rate image index -1 records what the form without it records, and another index is refused by name");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static RenderTexture Target(int depth = 0)
    {
        var texture = new RenderTexture(8, 8, depth, RenderTextureFormat.ARGB32);
        texture.Create();
        return texture;
    }

    private static AttachmentDescriptor Attachment(RenderTargetIdentifier target, RenderBufferLoadAction load) =>
        new(RenderTextureFormat.ARGB32)
        {
            loadStoreTarget = target,
            loadAction = load,
            storeAction = RenderBufferStoreAction.Store,
            clearColor = new Color(0.25f, 0.5f, 0.75f, 1f),
            clearDepth = 0.5f,
            clearStencil = 3,
        };

    private static SubPassDescriptor Outputs(params int[] indices)
    {
        var list = new AttachmentIndexArray(indices.Length);
        for (var i = 0; i < indices.Length; i++) list[i] = indices[i];
        return new SubPassDescriptor { colorOutputs = list };
    }

    private static string[] Run(CommandBuffer buffer)
    {
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        return CommandWireReader.Take();
    }

    private static void PassesBindAndClearAsTheirAttachmentsSay()
    {
        var (a, b, depth) = (Target(), Target(), Target(24));
        using var attachments = new NativeArray<AttachmentDescriptor>(
            [Attachment(a, RenderBufferLoadAction.Clear), Attachment(b, RenderBufferLoadAction.Load), Attachment(depth, RenderBufferLoadAction.Clear)],
            Allocator.Temp);
        using var subPasses = new NativeArray<SubPassDescriptor>([Outputs(0), Outputs(1, 0)], Allocator.Temp);
        var pass = new CommandBuffer();
        CommandBuffer.ThrowOnSetRenderTarget = true;
        try
        {
            pass.BeginRenderPass(8, 8, 1, 1, attachments, 2, subPasses, "pass"u8);
            pass.NextSubPass();
            pass.EndRenderPass();
        }
        finally { CommandBuffer.ThrowOnSetRenderTarget = false; }
        var expected = new CommandBuffer();
        expected.SetRenderTarget(a);
        expected.ClearRenderTarget(RTClearFlags.Color, new Color(0.25f, 0.5f, 0.75f, 1f), 1f, 0);
        expected.SetRenderTarget(Array.Empty<RenderTargetIdentifier>(), depth);
        expected.ClearRenderTarget(RTClearFlags.Depth | RTClearFlags.Stencil, default, 0.5f, 3);
        expected.SetRenderTarget([a], depth);
        expected.SetRenderTarget([b, a], depth);
        var want = Run(expected);
        var got = Run(pass);
        Expect.That(want.SequenceEqual(got), $"the pass records\n  {string.Join("\n  ", got)}\nwhere the explicit calls record\n  {string.Join("\n  ", want)}");

        using var noDepth = new NativeArray<AttachmentDescriptor>([Attachment(b, RenderBufferLoadAction.Load)], Allocator.Temp);
        using var one = new NativeArray<SubPassDescriptor>([Outputs(0)], Allocator.Temp);
        var plain = new CommandBuffer();
        plain.BeginRenderPass(8, 8, 1, noDepth, -1, one);
        plain.EndRenderPass();
        var bound = new CommandBuffer();
        bound.SetRenderTarget(b);
        Expect.That(Run(bound).SequenceEqual(Run(plain)), "a pass that loads one color attachment and has no depth binds it once and clears nothing");
    }

    private static void TheShadingRateFormWithNoImageIsTheFormWithout()
    {
        var (a, b, depth) = (Target(), Target(), Target(24));
        using var attachments = new NativeArray<AttachmentDescriptor>(
            [Attachment(a, RenderBufferLoadAction.Clear), Attachment(b, RenderBufferLoadAction.Load), Attachment(depth, RenderBufferLoadAction.Clear)],
            Allocator.Temp);
        using var subPasses = new NativeArray<SubPassDescriptor>([Outputs(0), Outputs(1, 0)], Allocator.Temp);
        var without = new CommandBuffer();
        without.BeginRenderPass(8, 8, 1, 1, attachments, 2, subPasses, "pass"u8);
        without.NextSubPass();
        without.EndRenderPass();
        var with = new CommandBuffer();
        with.BeginRenderPass(8, 8, 1, 1, attachments, 2, -1, subPasses, "pass"u8);
        with.NextSubPass();
        with.EndRenderPass();
        var want = Run(without);
        var got = Run(with);
        Expect.That(want.Length == 6 && want.SequenceEqual(got),
            $"index -1 records\n  {string.Join("\n  ", got)}\nwhere the form without it records\n  {string.Join("\n  ", want)}");

        var refused = new CommandBuffer();
        foreach (var index in new[] { 0, 1, 3 })
        {
            var error = Expect.Catch<NotSupportedException>(
                () => refused.BeginRenderPass(8, 8, 1, 1, attachments, 2, index, subPasses, "pass"u8), $"shading-rate image index {index}");
            Expect.That(error.Message.Contains("shadingRateImageAttachmentIndex") && error.Message.Contains($" {index} "),
                $"the refusal names the parameter and its value: {error.Message}");
        }
        Expect.Equal(0, refused.commandCount, "a refused pass records nothing");
        Expect.Throws<InvalidOperationException>(refused.EndRenderPass, "a refused pass leaves no pass open");

        var released = new CommandBuffer();
        released.Release();
        Expect.Throws<ObjectDisposedException>(
            () => released.BeginRenderPass(8, 8, 1, 1, attachments, 2, 0, subPasses, "pass"u8), "a released buffer, before the index");
    }

    private static void MisusedPassesAreRefused()
    {
        var target = Target();
        using var attachments = new NativeArray<AttachmentDescriptor>([Attachment(target, RenderBufferLoadAction.Load)], Allocator.Temp);
        using var one = new NativeArray<SubPassDescriptor>([Outputs(0)], Allocator.Temp);
        using var none = new NativeArray<SubPassDescriptor>(0, Allocator.Temp);
        using var stray = new NativeArray<SubPassDescriptor>([Outputs(1)], Allocator.Temp);
        var buffer = new CommandBuffer();
        Expect.Throws<InvalidOperationException>(buffer.NextSubPass, "a sub-pass with no pass open");
        Expect.Throws<InvalidOperationException>(buffer.EndRenderPass, "ending with no pass open");
        Expect.Throws<ArgumentOutOfRangeException>(() => buffer.BeginRenderPass(8, 8, 1, 1, attachments, 1, one), "a depth index past the attachments");
        Expect.Throws<ArgumentException>(() => buffer.BeginRenderPass(8, 8, 1, 1, attachments, -1, none), "no sub-passes");
        Expect.Throws<ArgumentException>(() => buffer.BeginRenderPass(8, 8, 1, 1, attachments, -1, stray), "an output past the attachments");
        Expect.Throws<ArgumentException>(() => buffer.BeginRenderPass(8, 8, 1, 1, attachments, 0, one), "the depth attachment as a color output");
        Expect.Equal(0, buffer.commandCount, "refused passes record nothing");
        buffer.BeginRenderPass(8, 8, 1, 1, attachments, -1, one);
        Expect.Throws<InvalidOperationException>(() => buffer.BeginRenderPass(8, 8, 1, 1, attachments, -1, one), "a pass inside a pass");
        Expect.Throws<InvalidOperationException>(buffer.NextSubPass, "a sub-pass past the last");
        buffer.EndRenderPass();
        buffer.BeginRenderPass(8, 8, 1, 1, attachments, -1, one);
        buffer.Clear();
        buffer.BeginRenderPass(8, 8, 1, 1, attachments, -1, one);
        Expect.Equal(1, buffer.commandCount, "Clear also drops the open pass");
    }
}
