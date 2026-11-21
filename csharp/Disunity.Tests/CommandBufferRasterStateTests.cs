using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>A pipeline's shadow caster pass sets a scissor rectangle and a global depth bias around its draws. Each call is
/// recorded in order and reaches the world when the buffer runs.</summary>
internal static class CommandBufferRasterStateTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ScissorAndBiasAreRecordedInOrder();
            Console.WriteLine("PASS: EnableScissorRect, DisableScissorRect and SetGlobalDepthBias record in call order and reach the world when the buffer runs");
            InvalidValuesAreRefusedBeforeRecording();
            Console.WriteLine("PASS: a scissor or bias that is not finite, or a negative size, throws naming the member and records nothing; a released buffer throws");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static void ScissorAndBiasAreRecordedInOrder()
    {
        var buffer = new CommandBuffer();
        buffer.SetGlobalDepthBias(1.25f, 2.5f);
        buffer.EnableScissorRect(new Rect(4, 5.5f, 56, 40.25f));
        buffer.DisableScissorRect();
        buffer.SetGlobalDepthBias(0f, 0f);
        Expect.Equal(4, buffer.commandCount, "each call records one command");
        Expect.Equal(0, CommandWireReader.Take().Length, "nothing reaches the world before the buffer runs");
        Graphics.ExecuteCommandBuffer(buffer);
        var taken = CommandWireReader.Take();
        Expect.That(
            new[] { "DepthBias bias=1.25 slope=2.5", "Scissor rect=(4 5.5 56 40.25)", "ScissorOff", "DepthBias bias=0 slope=0" }.SequenceEqual(taken),
            $"the records as the world holds them: {string.Join(" | ", taken)}");
        buffer.Release();
    }

    private static void InvalidValuesAreRefusedBeforeRecording()
    {
        foreach (var rect in new[]
        {
            new Rect(float.NaN, 0, 1, 1), new Rect(0, float.PositiveInfinity, 1, 1), new Rect(0, 0, float.NegativeInfinity, 1),
            new Rect(0, 0, 1, float.NaN), new Rect(0, 0, -1, 1), new Rect(0, 0, 1, -0.5f),
        })
        {
            var bad = new CommandBuffer();
            Throws<ArgumentException>(() => bad.EnableScissorRect(rect), $"the rectangle {rect}", "CommandBuffer.EnableScissorRect");
            Expect.Equal(0, bad.commandCount, "a refused rectangle records nothing");
        }
        foreach (var (bias, slope) in new[] { (float.NaN, 0f), (0f, float.PositiveInfinity), (float.NegativeInfinity, 1f) })
        {
            var bad = new CommandBuffer();
            Throws<ArgumentException>(() => bad.SetGlobalDepthBias(bias, slope), $"the bias {bias} {slope}", "CommandBuffer.SetGlobalDepthBias");
            Expect.Equal(0, bad.commandCount, "a refused bias records nothing");
        }
        var buffer = new CommandBuffer();
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.EnableScissorRect(new Rect(0, 0, 1, 1)), "a scissor on a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.DisableScissorRect(), "a scissor end on a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.SetGlobalDepthBias(1f, 1f), "a depth bias on a released buffer", "CommandBuffer");
    }
}
