using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>CommandBuffer.SetWireframe: turning wireframe off records nothing and leaves the buffer's other commands in place;
/// turning it on is refused by name, since the engine draws solid triangles only.</summary>
internal static class CommandBufferWireframeTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var target = new RenderTexture(8, 8, 0);
            target.Create();
            var buffer = new CommandBuffer();
            buffer.SetWireframe(false);
            buffer.SetRenderTarget(target);
            buffer.SetWireframe(false);
            CommandWireReader.Take();
            Graphics.ExecuteCommandBuffer(buffer);
            var taken = CommandWireReader.Take();
            Expect.Sequence(new[] { "SetRenderTarget" }, taken.Select(line => line.Split(' ')[0]),
                "SetWireframe(false) records nothing around the target");
            var refused = Expect.Catch<NotSupportedException>(() => buffer.SetWireframe(true), "wireframe on");
            Expect.That(refused.Message.Contains("CommandBuffer.SetWireframe", StringComparison.Ordinal),
                $"the message names the member: {refused.Message}");
            buffer.Release();
            Expect.Throws<ObjectDisposedException>(() => buffer.SetWireframe(false), "a released buffer");
            Console.WriteLine("PASS: CommandBuffer.SetWireframe(false) records nothing, SetWireframe(true) throws NotSupportedException naming the member, and a released buffer throws");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
