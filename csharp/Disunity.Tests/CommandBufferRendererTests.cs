using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>CommandBuffer.DrawRenderer through a real world and the public execution path: the renderer is read when the buffer
/// runs, so its transform, mesh and property block are the ones it has then, and the draw reaches the world as the matching
/// DrawMesh would. Only a MeshRenderer is drawn; what cannot be drawn is refused by name.</summary>
internal static class CommandBufferRendererTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ARendererIsDrawnAsItIsWhenTheBufferRuns();
            Console.WriteLine("PASS: CommandBuffer.DrawRenderer draws the renderer's mesh where the renderer is when the buffer runs, for each overload");
            TheRenderersPropertyBlockTravelsWithTheDraw();
            Console.WriteLine("PASS: CommandBuffer.DrawRenderer carries the property block the renderer holds when the buffer runs");
            WhatCannotBeDrawnIsRefusedByName();
            Console.WriteLine("PASS: CommandBuffer.DrawRenderer refuses a null or released input, a renderer that is not a MeshRenderer, and a renderer with no mesh or sub-mesh when the buffer runs");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static MeshRenderer Renderer(int subMeshes)
    {
        var mesh = new Mesh();
        mesh.vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0)];
        mesh.triangles = [0, 1, 2];
        mesh.subMeshCount = subMeshes;
        var host = new GameObject("drawn by a buffer");
        host.AddComponent<MeshFilter>().sharedMesh = mesh;
        return host.AddComponent<MeshRenderer>();
    }

    private static string[] Execute(CommandBuffer buffer)
    {
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        return CommandWireReader.Take();
    }

    private static string Line(MeshRenderer renderer, Material material, string matrix, (int SubMesh, int Pass) at, string block) =>
        $"DrawMesh mesh={renderer.GetComponent<MeshFilter>()!.sharedMesh!.RendererIdentity} matrix={matrix} material={material.WorldLink.Id} " +
        $"submesh={at.SubMesh} pass={at.Pass} block=[{block}]";

    private static void ARendererIsDrawnAsItIsWhenTheBufferRuns()
    {
        var renderer = Renderer(2);
        var material = new Material(Material.SpriteLit);
        renderer.transform.position = new Vector3(0, 0, -1);
        var buffer = new CommandBuffer();
        buffer.DrawRenderer(renderer, material);
        buffer.DrawRenderer(renderer, material, 1);
        buffer.DrawRenderer(renderer, material, 1, 3);
        renderer.transform.position = new Vector3(2, 0, 0);
        const string moved = "(1 0 0 0 0 1 0 0 0 0 1 0 2 0 0 1)";
        Expect.Sequence(new[]
        {
            Line(renderer, material, moved, (0, -1), "-"),
            Line(renderer, material, moved, (1, -1), "-"),
            Line(renderer, material, moved, (1, 3), "-"),
        }, Execute(buffer), "each overload draws the renderer's mesh at the position it has when the buffer runs");
    }

    private static void TheRenderersPropertyBlockTravelsWithTheDraw()
    {
        var renderer = Renderer(1);
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        buffer.DrawRenderer(renderer, material, 0, 0);
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 0.5f);
        renderer.SetPropertyBlock(block);
        const string identity = "(1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1)";
        Expect.Sequence(new[] { Line(renderer, material, identity, (0, 0), "_Fade=float (0.5)") }, Execute(buffer),
            "the block set on the renderer after recording is drawn");
        renderer.SetPropertyBlock(null);
        Expect.Sequence(new[] { Line(renderer, material, identity, (0, 0), "-") }, Execute(buffer),
            "a renderer with its block cleared draws with none");
    }

    private static void WhatCannotBeDrawnIsRefusedByName()
    {
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        Expect.Throws<ArgumentNullException>(() => buffer.DrawRenderer(null!, material), "a null renderer");
        Expect.Throws<ArgumentNullException>(() => buffer.DrawRenderer(Renderer(1), null!), "a null material");
        Expect.Throws<ArgumentException>(() => buffer.DrawRenderer(Renderer(1), material, -1), "a sub-mesh below 0");
        Expect.Throws<ArgumentException>(() => buffer.DrawRenderer(Renderer(1), material, 0, -2), "a pass below -1");
        var skinned = new GameObject("skinned").AddComponent<SkinnedMeshRenderer>();
        var unsupported = Expect.Catch<NotSupportedException>(() => buffer.DrawRenderer(skinned, material), "a skinned mesh renderer");
        Expect.That(unsupported.Message.Contains("CommandBuffer.DrawRenderer", StringComparison.Ordinal), $"the message names the member: {unsupported.Message}");
        Expect.Equal(0, Execute(buffer).Length, "refused calls record nothing");
        var emptied = Renderer(1);
        var noMesh = new CommandBuffer();
        noMesh.DrawRenderer(emptied, material);
        emptied.GetComponent<MeshFilter>()!.sharedMesh = null;
        var missing = Expect.Catch<ArgumentException>(() => Graphics.ExecuteCommandBuffer(noMesh), "a renderer whose mesh was removed");
        Expect.That(missing.Message.Contains("CommandBuffer.DrawRenderer: the renderer has no mesh", StringComparison.Ordinal), missing.Message);
        var outside = new CommandBuffer();
        outside.DrawRenderer(Renderer(1), material, 1);
        var lost = Expect.Catch<ArgumentException>(() => Graphics.ExecuteCommandBuffer(outside), "a sub-mesh the mesh does not have");
        Expect.That(lost.Message.Contains("CommandBuffer.DrawRenderer: sub-mesh 1", StringComparison.Ordinal), lost.Message);
        Expect.Equal(0, CommandWireReader.Take().Length, "a refused run records nothing");
        var released = new CommandBuffer();
        released.Release();
        Expect.Throws<ObjectDisposedException>(() => released.DrawRenderer(Renderer(1), material), "a released buffer");
    }
}
