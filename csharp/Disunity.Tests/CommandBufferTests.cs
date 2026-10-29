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
        Console.WriteLine("PASS: CommandBuffer name, Clear, Release, recording members and ComputeBuffer Release");
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
        RenderTargetIdentifier target = "_Target";
        var texture = new Texture2D(2, 2);
        var material = new Material(Material.SpriteLit);
        var probes = new (string Member, Action Call)[]
        {
            ("BeginSample", () => buffer.BeginSample("s")),
            ("EndSample", () => buffer.EndSample("s")),
            ("Blit", () => buffer.Blit(target, target)),
            ("Blit", () => buffer.Blit(texture, target)),
            ("Blit", () => buffer.Blit(target, target, material)),
            ("Blit", () => buffer.Blit(texture, target, material, 1)),
            ("ClearRenderTarget", () => buffer.ClearRenderTarget(true, true, Color.black)),
            ("ClearRenderTarget", () => buffer.ClearRenderTarget(true, true, Color.black, 1f)),
            ("CopyTexture", () => buffer.CopyTexture(target, 0, 0, target, 0, 0)),
            ("EnableShaderKeyword", () => buffer.EnableShaderKeyword("K")),
            ("DisableShaderKeyword", () => buffer.DisableShaderKeyword("K")),
            ("DrawMesh", () => buffer.DrawMesh(new Mesh(), Matrix4x4.identity, material, 0, 0)),
            ("DrawMeshInstancedIndirect", () => buffer.DrawMeshInstancedIndirect(new Mesh(), 0, material, 0, new ComputeBuffer(1, 4), 0, new MaterialPropertyBlock())),
            ("GetTemporaryRT", () => buffer.GetTemporaryRT(1, 4, 4, 0, FilterMode.Point, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Default)),
            ("ReleaseTemporaryRT", () => buffer.ReleaseTemporaryRT(1)),
            ("SetGlobalColor", () => buffer.SetGlobalColor(1, Color.white)),
            ("SetGlobalColor", () => buffer.SetGlobalColor("_C", Color.white)),
            ("SetGlobalFloat", () => buffer.SetGlobalFloat(1, 1f)),
            ("SetGlobalMatrix", () => buffer.SetGlobalMatrix(1, Matrix4x4.identity)),
            ("SetGlobalTexture", () => buffer.SetGlobalTexture(1, target)),
            ("SetGlobalTexture", () => buffer.SetGlobalTexture("_T", target)),
            ("SetGlobalVector", () => buffer.SetGlobalVector(1, Vector4.zero)),
            ("SetGlobalVector", () => buffer.SetGlobalVector("_V", Vector4.zero)),
            ("SetRenderTarget", () => buffer.SetRenderTarget(target)),
            ("SetRenderTarget", () => buffer.SetRenderTarget(new[] { target }, target)),
            ("SetViewport", () => buffer.SetViewport(new Rect(0, 0, 1, 1))),
            ("SetViewProjectionMatrices", () => buffer.SetViewProjectionMatrices(Matrix4x4.identity, Matrix4x4.identity)),
        };
        foreach (var (member, call) in probes)
            Throws<NotSupportedException>(call, member, $"CommandBuffer.{member}", "does not record command buffers yet");
    }

    private static void Compute()
    {
        var buffer = new ComputeBuffer(4, 16, ComputeBufferType.Structured, ComputeBufferMode.Immutable);
        Ensure(buffer.count == 4, "count");
        Ensure(new ComputeBuffer(2, 4).count == 2 && new ComputeBuffer(3, 4, ComputeBufferType.Raw).count == 3, "the other constructors");
        Throws<ArgumentException>(() => new ComputeBuffer(0, 4), "a zero count");
        Throws<ArgumentException>(() => new ComputeBuffer(4, 0), "a zero stride");
        Throws<NotSupportedException>(() => buffer.GetData(new int[4]), "GetData", "ComputeBuffer.GetData");
        Throws<NotSupportedException>(() => buffer.SetData(new int[4]), "SetData", "ComputeBuffer.SetData");
        buffer.Release();
        buffer.Release();
        buffer.Dispose();
        Ensure(buffer.count == 4, "count survives Release");
        Throws<ObjectDisposedException>(() => buffer.SetData(new int[4]), "SetData after Release");
    }
}
