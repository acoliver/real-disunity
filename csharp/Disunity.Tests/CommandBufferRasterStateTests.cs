using UnityEngine.Rendering;

/// <summary>A pipeline's shadow caster pass sets a global depth bias around its draws; the engine accepts the call and records nothing.</summary>
internal static class CommandBufferDepthBiasTests
{
    public static void Run()
    {
        var buffer = new CommandBuffer();
        buffer.SetGlobalDepthBias(1.25f, 2.5f);
        buffer.SetGlobalDepthBias(0f, 0f);
        Expect.Equal(0, buffer.commandCount, "the global depth bias records nothing");
        buffer.Release();
        Expect.Throws<ObjectDisposedException>(() => buffer.SetGlobalDepthBias(1f, 1f), "a depth bias on a released buffer");
        Console.WriteLine("PASS: SetGlobalDepthBias is accepted on a live command buffer, records nothing and is refused on a released one");
    }
}
