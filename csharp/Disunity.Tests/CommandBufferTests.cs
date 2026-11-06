using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>CommandBuffer name and lifetime, its recording members, and ComputeBuffer release.</summary>
internal static class CommandBufferTests
{
    public static void Run()
    {
        Naming();
        Lifetime();
        Recording();
        Compute();
        Console.WriteLine("PASS: CommandBuffer name, Clear, Release, the members that cannot record yet and ComputeBuffer Release");
    }

    private static void Naming()
    {
        var buffer = new CommandBuffer();
        Ensure(buffer.name == "", "the name defaults to empty");
        buffer.name = "Outline";
        Ensure(buffer.name == "Outline", "the name is settable");
        buffer.name = null!;
        Ensure(buffer.name == "", "a null name becomes empty");
        Ensure(ModuleFacades.Forwards(typeof(DisunityHost).Assembly, "UnityEngine.Rendering.CommandBuffer"), "the module facades forward CommandBuffer");
    }

    private static void Lifetime()
    {
        var buffer = new CommandBuffer { name = "kept" };
        buffer.Clear();
        buffer.Release();
        buffer.Release();
        buffer.name = "after";
        Ensure(buffer.name == "after", "the name works after Release");
        Throws<ObjectDisposedException>(buffer.Clear, "Clear after Release");
        Throws<ObjectDisposedException>(() => buffer.BeginSample("x"), "BeginSample after Release");
        Throws<ObjectDisposedException>(() => buffer.SetViewport(default), "SetViewport after Release");
    }

    private static void Recording()
    {
        var buffer = new CommandBuffer();
        var material = new Material(Material.SpriteLit);
        var probes = new (string Member, Action Call)[]
        {
            ("BeginSample", () => buffer.BeginSample("s")),
            ("EndSample", () => buffer.EndSample("s")),
            ("DrawMesh", () => buffer.DrawMesh(new Mesh(), Matrix4x4.identity, material, 0, 0)),
            ("DrawMeshInstancedIndirect", () => buffer.DrawMeshInstancedIndirect(new Mesh(), 0, material, 0, new ComputeBuffer(1, 4), 0, new MaterialPropertyBlock())),
            ("SetViewport", () => buffer.SetViewport(new Rect(0, 0, 1, 1))),
            ("SetViewProjectionMatrices", () => buffer.SetViewProjectionMatrices(Matrix4x4.identity, Matrix4x4.identity)),
        };
        foreach (var (member, call) in probes)
            Throws<NotSupportedException>(call, member, $"CommandBuffer.{member}", "cannot record this member yet");
        Ensure(buffer.commandCount == 0, "members that cannot record add nothing to the buffer");
    }

    private static void Compute()
    {
        var buffer = new ComputeBuffer(4, 16, ComputeBufferType.Structured, ComputeBufferMode.Immutable);
        Ensure(buffer.count == 4, "count");
        Ensure(new ComputeBuffer(2, 4).count == 2 && new ComputeBuffer(3, 4, ComputeBufferType.Raw).count == 3, "the other constructors");
        Throws<ArgumentException>(() => new ComputeBuffer(0, 4), "a zero count");
        Throws<ArgumentException>(() => new ComputeBuffer(4, 0), "a zero stride");
        buffer.Release();
        buffer.Release();
        buffer.Dispose();
        Ensure(buffer.count == 4, "count survives Release");
        Throws<ObjectDisposedException>(() => buffer.SetData(new int[4]), "SetData after Release");
    }
}
