using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>CommandBuffer.DrawProcedural without an index buffer records a draw of generated vertices in order with the buffer's
/// other commands, through a real world and the public execution paths. The indexed forms, a property block that holds values,
/// and topologies other than triangles and lines throw <see cref="NotSupportedException"/> naming the member and record nothing.</summary>
internal static class CommandBufferProceduralTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            EachNonIndexedFormRecordsItsDrawWithDefaults();
            NothingReachesTheWorldBeforeExecutionAndRepeatsAreEqual();
            ContextExecutionWaitsForSubmit();
            ProceduralDrawsKeepOrderWithGlobalsAndOtherDraws();
            InvalidInputIsRefusedWhenCalled();
            UnsupportedFormsNameThemselvesAndRecordNothing();
            AMaterialDestroyedAfterRecordingIsRefusedWhenTheBufferRuns();
            AReleasedBufferIsRefusedFirst();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static readonly Matrix4x4 Shear = new(
        new Vector4(2, 0, 0, 0), new Vector4(5, 3, 0, 0), new Vector4(0, 0, 4, 0), new Vector4(7, 8, 9, 1));

    private const string ShearColumns = "(2 0 0 0 5 3 0 0 0 0 4 0 7 8 9 1)";

    private static string Line(ulong material, int pass, int topology, int vertices, int instances) =>
        $"DrawProcedural matrix={ShearColumns} material={material} pass={pass} topology={topology} vertices={vertices} instances={instances}";

    private static string[] Execute(CommandBuffer buffer)
    {
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        return CommandWireReader.Take();
    }

    private static void EachNonIndexedFormRecordsItsDrawWithDefaults()
    {
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        buffer.DrawProcedural(Shear, material, 1, MeshTopology.Triangles, 6, 2, new MaterialPropertyBlock());
        buffer.DrawProcedural(Shear, material, -1, MeshTopology.Lines, 4, 3);
        buffer.DrawProcedural(Shear, material, 0, MeshTopology.Triangles, 3);
        buffer.DrawProcedural(Shear, material, 0, MeshTopology.Triangles, 3, 1, null);
        var id = material.WorldLink.Id;
        Expect.Sequence(new[]
        {
            Line(id, 1, 0, 6, 2), Line(id, -1, 1, 4, 3), Line(id, 0, 0, 3, 1), Line(id, 0, 0, 3, 1),
        }, Execute(buffer), "each form records matrix, material, pass, topology and counts; instance count defaults to one");
        Console.WriteLine("PASS: every non-indexed DrawProcedural form records its draw, with one instance when the count is omitted");
    }

    private static void NothingReachesTheWorldBeforeExecutionAndRepeatsAreEqual()
    {
        var material = new Material(Material.SpriteLit);
        CommandWireReader.Take();
        var buffer = new CommandBuffer();
        buffer.DrawProcedural(Shear, material, 0, MeshTopology.Triangles, 0, 1);
        buffer.DrawProcedural(Shear, material, 0, MeshTopology.Lines, 2, 0);
        Ensure(CommandWireReader.Take().Length == 0, "recording sends the world nothing");
        Ensure(buffer.commandCount == 2, "both draws, including the empty ones, are commands");
        var id = material.WorldLink.Id;
        var expected = new[] { Line(id, 0, 0, 0, 1), Line(id, 0, 1, 2, 0) };
        Expect.Sequence(expected, Execute(buffer), "zero vertex or instance counts are recorded in order");
        Expect.Sequence(expected, Execute(buffer), "a second execution sends the same records");
        buffer.Clear();
        Ensure(Execute(buffer).Length == 0, "a cleared buffer sends nothing");
        Console.WriteLine("PASS: procedural draws reach the world only when the buffer runs, repeat exactly, and empty draws keep their place");
    }

    private static void ContextExecutionWaitsForSubmit()
    {
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        buffer.DrawProcedural(Shear, material, 0, MeshTopology.Lines, 2);
        var context = new ScriptableRenderContext();
        CommandWireReader.Take();
        context.ExecuteCommandBuffer(buffer);
        Ensure(CommandWireReader.Take().Length == 0, "context execution queues until Submit");
        context.Submit();
        Expect.Sequence(new[] { Line(material.WorldLink.Id, 0, 1, 2, 1) }, CommandWireReader.Take(), "Submit sends the queued draw");
        Console.WriteLine("PASS: ScriptableRenderContext.ExecuteCommandBuffer delivers the procedural draw at Submit");
    }

    private static void ProceduralDrawsKeepOrderWithGlobalsAndOtherDraws()
    {
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        buffer.SetGlobalVectorArray("_Pv", new[] { new Vector4(1, 2, 3, 4) });
        buffer.DrawProcedural(Shear, material, 0, MeshTopology.Triangles, 3);
        buffer.SetGlobalVectorArray("_Pv", new[] { new Vector4(5, 6, 7, 8) });
        buffer.DrawProcedural(Shear, material, 0, MeshTopology.Lines, 2);
        var records = Execute(buffer);
        Ensure(records.Length == 4, $"four records, got {records.Length}");
        Ensure(records[0].StartsWith("SetGlobal _Pv") && records[2].StartsWith("SetGlobal _Pv"), "globals at their places");
        Ensure(records[1] == Line(material.WorldLink.Id, 0, 0, 3, 1) && records[3] == Line(material.WorldLink.Id, 0, 1, 2, 1), "draws between them");
        Console.WriteLine("PASS: procedural draws keep their order between global array writes");
    }

    private static void InvalidInputIsRefusedWhenCalled()
    {
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        Throws<ArgumentNullException>(() => buffer.DrawProcedural(Shear, null!, 0, MeshTopology.Triangles, 3), "a null material", "material");
        Throws<ArgumentException>(() => buffer.DrawProcedural(Shear, material, -2, MeshTopology.Triangles, 3), "pass -2", "below -1");
        Throws<ArgumentOutOfRangeException>(() => buffer.DrawProcedural(Shear, material, 0, MeshTopology.Triangles, -1), "negative vertices", "vertexCount");
        Throws<ArgumentOutOfRangeException>(() => buffer.DrawProcedural(Shear, material, 0, MeshTopology.Triangles, 3, -1), "negative instances", "instanceCount");
        foreach (var bad in new[] { float.NaN, float.PositiveInfinity, float.NegativeInfinity })
        {
            var matrix = Shear;
            matrix[2, 3] = bad;
            Throws<ArgumentException>(() => buffer.DrawProcedural(matrix, material, 0, MeshTopology.Triangles, 3), $"a {bad} matrix component", "not finite");
        }
        var gone = new Material(Material.SpriteLit);
        UnityEngine.Object.DestroyImmediate(gone);
        Throws<ArgumentException>(() => buffer.DrawProcedural(Shear, gone, 0, MeshTopology.Triangles, 3), "a destroyed material", "destroyed");
        Ensure(buffer.commandCount == 0, "refused calls record nothing");
        Console.WriteLine("PASS: a null or destroyed material, a pass below -1, a matrix component that is not finite and negative counts are refused when called");
    }

    private static void UnsupportedFormsNameThemselvesAndRecordNothing()
    {
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        var block = new MaterialPropertyBlock();
        block.SetFloat("_X", 1);
        using var indices = new GraphicsBuffer(GraphicsBuffer.Target.Index, 3, 4);
        var calls = new Action[]
        {
            () => buffer.DrawProcedural(Shear, material, 0, MeshTopology.Triangles, 3, 1, block),
            () => buffer.DrawProcedural(Shear, material, 0, MeshTopology.Quads, 4),
            () => buffer.DrawProcedural(Shear, material, 0, MeshTopology.LineStrip, 4),
            () => buffer.DrawProcedural(Shear, material, 0, MeshTopology.Points, 4),
            () => buffer.DrawProcedural(indices, Shear, material, 0, MeshTopology.Triangles, 3, 1, null!),
            () => buffer.DrawProcedural(indices, Shear, material, 0, MeshTopology.Triangles, 3, 1),
            () => buffer.DrawProcedural(indices, Shear, material, 0, MeshTopology.Triangles, 3),
        };
        foreach (var call in calls) Throws<NotSupportedException>(call, "an unsupported DrawProcedural form", "CommandBuffer.DrawProcedural");
        Ensure(buffer.commandCount == 0, "unsupported forms record nothing");
        Ensure(Execute(buffer).Length == 0, "and send nothing");
        Console.WriteLine("PASS: indexed forms, populated property blocks and other topologies throw NotSupportedException naming the member");
    }

    private static void AMaterialDestroyedAfterRecordingIsRefusedWhenTheBufferRuns()
    {
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        buffer.DrawProcedural(Shear, material, 0, MeshTopology.Triangles, 3);
        UnityEngine.Object.DestroyImmediate(material);
        CommandWireReader.Take();
        Throws<ArgumentException>(() => Graphics.ExecuteCommandBuffer(buffer), "a material destroyed after recording", "destroyed");
        Ensure(CommandWireReader.Take().Length == 0, "the refused draw sends nothing");
        Console.WriteLine("PASS: a material destroyed after recording is refused when the buffer runs");
    }

    private static void AReleasedBufferIsRefusedFirst()
    {
        var buffer = new CommandBuffer();
        buffer.Release();
        using var indices = new GraphicsBuffer(GraphicsBuffer.Target.Index, 3, 4);
        Throws<ObjectDisposedException>(() => buffer.DrawProcedural(Shear, null!, -5, MeshTopology.Points, -1), "released, invalid", "CommandBuffer");
        var unfinite = Shear;
        unfinite[0, 0] = float.NaN;
        Throws<ObjectDisposedException>(() => buffer.DrawProcedural(unfinite, new Material(Material.SpriteLit), 0, MeshTopology.Triangles, 3),
            "released, matrix not finite", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.DrawProcedural(indices, Shear, null!, 0, MeshTopology.Triangles, 3), "released, indexed", "CommandBuffer");
        Console.WriteLine("PASS: a released buffer throws ObjectDisposedException before any other check");
    }
}
