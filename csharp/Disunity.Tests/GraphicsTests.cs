using System.Reflection;
using Unity.Collections;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The static Graphics class holds engine-wide values and refuses every call that would draw or run GPU work.</summary>
internal static class GraphicsTests
{
    public static void Run()
    {
        Values();
        Console.WriteLine("PASS: Graphics reports tier 3, sRGB, no OpenGL ES and no framebuffer alpha, and stores the active tier");
        var calls = Calls();
        EveryCallThrowsNamingItself(calls);
        EveryOverloadIsCovered(calls);
        Console.WriteLine("PASS: every Graphics rendering entry point throws NotSupportedException naming itself");
    }

    private static void Values()
    {
        Expect.Equal(GraphicsTier.Tier3, Graphics.activeTier, "activeTier");
        Expect.Equal(OpenGLESVersion.None, Graphics.minOpenGLESVersion, "minOpenGLESVersion");
        Expect.That(!Graphics.preserveFramebufferAlpha, "preserveFramebufferAlpha");
        Expect.Equal(ColorGamut.sRGB, Graphics.activeColorGamut, "activeColorGamut");
        Expect.Equal(default(RenderBuffer), Graphics.activeColorBuffer, "activeColorBuffer binds nothing");
        Expect.Equal(default(RenderBuffer), Graphics.activeDepthBuffer, "activeDepthBuffer binds nothing");
        Graphics.activeTier = GraphicsTier.Tier1;
        Expect.Equal(GraphicsTier.Tier1, Graphics.activeTier, "activeTier set");
        Graphics.activeTier = GraphicsTier.Tier3;
        Expect.Equal(GraphicsTier.Tier3, Graphics.activeTier, "activeTier restored");
    }

    private static void EveryCallThrowsNamingItself(List<(string Member, Action Call)> calls)
    {
        foreach (var (member, call) in calls)
        {
            var error = Expect.Catch<NotSupportedException>(call, member);
            Expect.That(error.Message.StartsWith($"Graphics.{member}:", StringComparison.Ordinal), $"the message names {member}: {error.Message}");
        }
    }

    /// <summary>The render texture overloads of SetRenderTarget work, and Blit, BlitMultiTap and CopyTexture
    /// complete without drawing; GraphicsTargetTests and GraphicsBlitTests cover them.</summary>
    private static bool DrawsNothingButSetsTheActiveTexture(MethodInfo method) =>
        (method.Name == "SetRenderTarget" && method.GetParameters()[0].ParameterType == typeof(RenderTexture))
        || method.Name is "Blit" or "BlitMultiTap" or "CopyTexture";

    private static void EveryOverloadIsCovered(List<(string Member, Action Call)> calls)
    {
        var declared = typeof(Graphics).GetMethods(BindingFlags.Public | BindingFlags.Static | BindingFlags.DeclaredOnly)
            .Where(m => !m.IsSpecialName && !DrawsNothingButSetsTheActiveTexture(m))
            .GroupBy(m => m.Name)
            .ToDictionary(g => g.Key, g => g.Count());
        var tested = calls.GroupBy(c => c.Member).ToDictionary(g => g.Key, g => g.Count());
        foreach (var (name, count) in declared)
            Expect.Equal(count, tested.GetValueOrDefault(name), $"Graphics.{name} overloads tested");
        foreach (var name in tested.Keys)
            Expect.That(declared.ContainsKey(name), $"{name} is declared");
    }

    private static List<(string Member, Action Call)> Calls()
    {
        var calls = new List<(string, Action)>();
        AddCopies(calls);
        AddTargets(calls);
        AddMeshDraws(calls);
        AddProceduralDraws(calls);
        AddRenderParamsDraws(calls);
        return calls;
    }

    private static void AddCopies(List<(string, Action)> calls)
    {
        Texture t = null!;
        GraphicsBuffer gb = null!;
        Rect rect = default;
        calls.AddRange(new (string, Action)[]
        {
            ("ConvertTexture", () => Graphics.ConvertTexture(t, t)),
            ("ConvertTexture", () => Graphics.ConvertTexture(t, 0, t, 0)),
            ("CopyBuffer", () => Graphics.CopyBuffer(gb, gb)),
            ("DrawTexture", () => Graphics.DrawTexture(rect, t)),
            ("DrawTexture", () => Graphics.DrawTexture(rect, t, 1, 1, 1, 1)),
            ("DrawTexture", () => Graphics.DrawTexture(rect, t, rect, 1, 1, 1, 1)),
            ("DrawTexture", () => Graphics.DrawTexture(rect, t, 1, 1, 1, 1, Color.white)),
            ("DrawTexture", () => Graphics.DrawTexture(rect, t, rect, 1, 1, 1, 1, Color.white)),
        });
    }

    private static void AddTargets(List<(string, Action)> calls)
    {
        RenderTexture rt = null!;
        RenderBuffer color = default;
        ComputeBuffer cb = null!;
        GraphicsBuffer gb = null!;
        CommandBuffer commands = null!;
        calls.AddRange(new (string, Action)[]
        {
            ("SetRenderTarget", () => Graphics.SetRenderTarget(color, color)),
            ("SetRenderTarget", () => Graphics.SetRenderTarget(color, color, 1)),
            ("SetRenderTarget", () => Graphics.SetRenderTarget(color, color, 1, CubemapFace.PositiveX)),
            ("SetRenderTarget", () => Graphics.SetRenderTarget(color, color, 1, CubemapFace.PositiveX, 2)),
            ("SetRenderTarget", () => Graphics.SetRenderTarget(new[] { color }, color)),
            ("SetRenderTarget", () => Graphics.SetRenderTarget(new RenderTargetSetup(color, color))),
            ("SetRandomWriteTarget", () => Graphics.SetRandomWriteTarget(1, rt)),
            ("SetRandomWriteTarget", () => Graphics.SetRandomWriteTarget(1, cb)),
            ("SetRandomWriteTarget", () => Graphics.SetRandomWriteTarget(1, gb)),
            ("ClearRandomWriteTargets", () => Graphics.ClearRandomWriteTargets()),
            ("ExecuteCommandBuffer", () => Graphics.ExecuteCommandBuffer(commands)),
            ("ExecuteCommandBufferAsync", () => Graphics.ExecuteCommandBufferAsync(commands, ComputeQueueType.Default)),
            ("CreateGraphicsFence", () => Graphics.CreateGraphicsFence(GraphicsFenceType.CPUSynchronisation)),
            ("CreateAsyncGraphicsFence", () => Graphics.CreateAsyncGraphicsFence()),
            ("WaitOnAsyncGraphicsFence", () => Graphics.WaitOnAsyncGraphicsFence(default, SynchronisationStage.VertexProcessing)),
        });
    }

    private static void AddMeshDraws(List<(string, Action)> calls)
    {
        Mesh mesh = null!;
        Material m = null!;
        Camera cam = null!;
        MaterialPropertyBlock block = null!;
        Transform anchor = null!;
        ComputeBuffer cb = null!;
        GraphicsBuffer gb = null!;
        var matrix = Matrix4x4.identity;
        var bounds = new Bounds(Vector3.zero, Vector3.one);
        var pos = Vector3.zero;
        var rot = Quaternion.identity;
        calls.AddRange(new (string, Action)[]
        {
            ("DrawMesh", () => Graphics.DrawMesh(mesh, pos, rot, m, 0)),
            ("DrawMesh", () => Graphics.DrawMesh(mesh, matrix, m, 0, cam)),
            ("DrawMesh", () => Graphics.DrawMesh(mesh, pos, rot, m, 0, cam, 0, block, ShadowCastingMode.On)),
            ("DrawMesh", () => Graphics.DrawMesh(mesh, matrix, m, 0, cam, 0, block, ShadowCastingMode.On)),
            ("DrawMesh", () => Graphics.DrawMesh(mesh, pos, rot, m, 0, cam, 0, block, ShadowCastingMode.On, true, anchor, LightProbeUsage.Off)),
            ("DrawMesh", () => Graphics.DrawMesh(mesh, matrix, m, 0, cam, 0, block, ShadowCastingMode.On, true, anchor, LightProbeUsage.Off)),
            ("DrawMeshNow", () => Graphics.DrawMeshNow(mesh, pos, rot)),
            ("DrawMeshNow", () => Graphics.DrawMeshNow(mesh, pos, rot, 1)),
            ("DrawMeshNow", () => Graphics.DrawMeshNow(mesh, matrix)),
            ("DrawMeshNow", () => Graphics.DrawMeshNow(mesh, matrix, 1)),
            ("DrawMeshInstanced", () => Graphics.DrawMeshInstanced(mesh, 0, m, new Matrix4x4[1])),
            ("DrawMeshInstanced", () => Graphics.DrawMeshInstanced(mesh, 0, m, new List<Matrix4x4>())),
            ("DrawMeshInstancedProcedural", () => Graphics.DrawMeshInstancedProcedural(mesh, 0, m, bounds, 4)),
            ("DrawMeshInstancedIndirect", () => Graphics.DrawMeshInstancedIndirect(mesh, 0, m, bounds, cb)),
            ("DrawMeshInstancedIndirect", () => Graphics.DrawMeshInstancedIndirect(mesh, 0, m, bounds, gb)),
        });
    }

    private static void AddProceduralDraws(List<(string, Action)> calls)
    {
        Material m = null!;
        ComputeBuffer cb = null!;
        GraphicsBuffer gb = null!;
        var bounds = new Bounds(Vector3.zero, Vector3.one);
        calls.AddRange(new (string, Action)[]
        {
            ("DrawProcedural", () => Graphics.DrawProcedural(m, bounds, MeshTopology.Triangles, 3)),
            ("DrawProcedural", () => Graphics.DrawProcedural(m, bounds, MeshTopology.Triangles, gb, 3)),
            ("DrawProceduralNow", () => Graphics.DrawProceduralNow(MeshTopology.Triangles, 3)),
            ("DrawProceduralNow", () => Graphics.DrawProceduralNow(MeshTopology.Triangles, gb, 3)),
            ("DrawProceduralIndirect", () => Graphics.DrawProceduralIndirect(m, bounds, MeshTopology.Triangles, cb)),
            ("DrawProceduralIndirect", () => Graphics.DrawProceduralIndirect(m, bounds, MeshTopology.Triangles, gb)),
            ("DrawProceduralIndirect", () => Graphics.DrawProceduralIndirect(m, bounds, MeshTopology.Triangles, gb, gb)),
            ("DrawProceduralIndirectNow", () => Graphics.DrawProceduralIndirectNow(MeshTopology.Triangles, cb)),
            ("DrawProceduralIndirectNow", () => Graphics.DrawProceduralIndirectNow(MeshTopology.Triangles, gb)),
            ("DrawProceduralIndirectNow", () => Graphics.DrawProceduralIndirectNow(MeshTopology.Triangles, gb, gb)),
        });
    }

    private static void AddRenderParamsDraws(List<(string, Action)> calls)
    {
        Mesh mesh = null!;
        GraphicsBuffer gb = null!;
        var p = new RenderParams(null!);
        var matrix = Matrix4x4.identity;
        calls.AddRange(new (string, Action)[]
        {
            ("RenderMesh", () => Graphics.RenderMesh(in p, mesh, 0, matrix)),
            ("RenderMeshInstanced", () => Graphics.RenderMeshInstanced(in p, mesh, 0, new Matrix4x4[1])),
            ("RenderMeshInstanced", () => Graphics.RenderMeshInstanced(in p, mesh, 0, new List<Matrix4x4>())),
            ("RenderMeshInstanced", () => Graphics.RenderMeshInstanced(in p, mesh, 0, default(NativeArray<Matrix4x4>))),
            ("RenderMeshIndirect", () => Graphics.RenderMeshIndirect(in p, mesh, gb)),
            ("RenderMeshPrimitives", () => Graphics.RenderMeshPrimitives(in p, mesh, 0)),
            ("RenderPrimitives", () => Graphics.RenderPrimitives(in p, MeshTopology.Triangles, 3)),
            ("RenderPrimitivesIndexed", () => Graphics.RenderPrimitivesIndexed(in p, MeshTopology.Triangles, gb, 3)),
            ("RenderPrimitivesIndirect", () => Graphics.RenderPrimitivesIndirect(in p, MeshTopology.Triangles, gb)),
            ("RenderPrimitivesIndexedIndirect", () => Graphics.RenderPrimitivesIndexedIndirect(in p, MeshTopology.Triangles, gb, gb)),
        });
    }
}
