using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>CommandBuffer.DrawMeshInstancedIndirect records one DrawMesh whose instance count is the second of the five argument
/// ints the args buffer holds when the command buffer runs, through a real world and the public execution paths. The call
/// refuses what DrawMesh refuses and an args buffer or offset that cannot hold five ints; when the buffer runs it refuses
/// arguments that do not draw one whole sub-mesh from instance 0, and arguments a GPU kernel may have written.</summary>
internal static class CommandBufferInstancedIndirectTests
{
    private const string Member = "CommandBuffer.DrawMeshInstancedIndirect";
    private const string Identity = "(1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1)";

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            TheInstanceCountOfTheArgsIsRecorded();
            ArgsAtAnOffsetAreRead();
            TheArgsAreReadWhenTheBufferRuns();
            AZeroCountKeepsItsPlace();
            ASubMeshOtherThanTheFirstIsDrawn();
            APropertyBlockTravelsWithTheDraw();
            TheCallRefusesWhatDrawMeshRefuses();
            TheCallRefusesAnArgsBufferOrOffsetThatCannotHoldFiveInts();
            ArgsThatDoNotDrawTheWholeSubMeshAreRefusedWhenTheBufferRuns();
            ArgsAKernelMayHaveWrittenAreRefusedWhenTheBufferRuns();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static Mesh Quads(int subMeshes = 1)
    {
        var mesh = new Mesh();
        mesh.vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0), new Vector3(1, 1, 0)];
        mesh.subMeshCount = subMeshes;
        mesh.SetTriangles(new[] { 0, 1, 2 }, 0);
        if (subMeshes > 1) mesh.SetTriangles(new[] { 1, 3, 2, 0, 1, 3 }, 1);
        return mesh;
    }

    /// <summary>The five ints a draw of the whole sub-mesh with <paramref name="instances"/> instances holds.</summary>
    private static int[] ArgsOf(Mesh mesh, int submesh, int instances) =>
        [(int)mesh.GetIndexCount(submesh), instances, (int)mesh.GetIndexStart(submesh), unchecked((int)mesh.GetBaseVertex(submesh)), 0];

    private static ComputeBuffer ArgsBuffer(params int[] ints)
    {
        var buffer = new ComputeBuffer(ints.Length / 5, 5 * 4, ComputeBufferType.DrawIndirect);
        buffer.SetData(ints);
        return buffer;
    }

    private static string Line(Mesh mesh, Material material, string suffix, int submesh = 0, int pass = 0, string block = "-") =>
        $"DrawMesh mesh={mesh.RendererIdentity} matrix={Identity} material={material.WorldLink.Id} submesh={submesh} pass={pass}{suffix} block=[{block}]";

    private static string[] Execute(CommandBuffer buffer)
    {
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        return CommandWireReader.Take();
    }

    private static void TheInstanceCountOfTheArgsIsRecorded()
    {
        var (mesh, material) = (Quads(), new Material(Material.SpriteLit));
        using var args = ArgsBuffer(ArgsOf(mesh, 0, 7));
        var buffer = new CommandBuffer();
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, 0, null!);
        Expect.Equal(1, buffer.commandCount, "the call records one command");
        Expect.Sequence(new[] { Line(mesh, material, " instances=7") }, Execute(buffer), "a DrawMesh of 7 instances");
        Console.WriteLine("PASS: CommandBuffer.DrawMeshInstancedIndirect records a DrawMesh whose instance count is the args buffer's");
    }

    private static void ArgsAtAnOffsetAreRead()
    {
        var (mesh, material) = (Quads(), new Material(Material.SpriteLit));
        using var args = ArgsBuffer([.. ArgsOf(mesh, 0, 3), .. ArgsOf(mesh, 0, 9)]);
        var buffer = new CommandBuffer();
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, -1, args, 20, new MaterialPropertyBlock());
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, -1, args, 0, null!);
        Expect.Sequence(new[] { Line(mesh, material, " instances=9", pass: -1), Line(mesh, material, " instances=3", pass: -1) }, Execute(buffer),
            "each call reads the five ints at its own byte offset");
        Console.WriteLine("PASS: DrawMeshInstancedIndirect reads the five ints at the byte offset it is given");
    }

    private static void TheArgsAreReadWhenTheBufferRuns()
    {
        var (mesh, material) = (Quads(), new Material(Material.SpriteLit));
        using var args = ArgsBuffer(ArgsOf(mesh, 0, 2));
        var buffer = new CommandBuffer();
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, 0, null!);
        args.SetData(ArgsOf(mesh, 0, 11));
        Expect.Sequence(new[] { Line(mesh, material, " instances=11") }, Execute(buffer), "the count written after the call");
        args.SetData(ArgsOf(mesh, 0, 4));
        Expect.Sequence(new[] { Line(mesh, material, " instances=4") }, Execute(buffer), "a buffer that runs again reads the args again");
        Console.WriteLine("PASS: DrawMeshInstancedIndirect reads the args buffer each time the command buffer runs");
    }

    private static void AZeroCountKeepsItsPlace()
    {
        var (mesh, material) = (Quads(), new Material(Material.SpriteLit));
        using var args = ArgsBuffer(ArgsOf(mesh, 0, 0));
        var buffer = new CommandBuffer();
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0);
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, 0, null!);
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0);
        Expect.Sequence(new[] { Line(mesh, material, ""), Line(mesh, material, " instances=0"), Line(mesh, material, "") }, Execute(buffer),
            "a draw of no instances stays between the draws around it");
        Console.WriteLine("PASS: a DrawMeshInstancedIndirect of zero instances records a draw of nothing in its place");
    }

    private static void ASubMeshOtherThanTheFirstIsDrawn()
    {
        var (mesh, material) = (Quads(2), new Material(Material.SpriteLit));
        Expect.Equal(3u, mesh.GetIndexStart(1), "the second sub-mesh starts after the first");
        using var args = ArgsBuffer(ArgsOf(mesh, 1, 5));
        var buffer = new CommandBuffer();
        buffer.DrawMeshInstancedIndirect(mesh, 1, material, 2, args, 0, null!);
        Expect.Sequence(new[] { Line(mesh, material, " instances=5", submesh: 1, pass: 2) }, Execute(buffer), "the draw names sub-mesh 1");
        Console.WriteLine("PASS: DrawMeshInstancedIndirect checks the args against the sub-mesh it names");
    }

    private static void APropertyBlockTravelsWithTheDraw()
    {
        var (mesh, material) = (Quads(), new Material(Material.SpriteLit));
        using var args = ArgsBuffer(ArgsOf(mesh, 0, 6));
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 0.5f);
        var buffer = new CommandBuffer();
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, 0, block);
        block.SetFloat("_Fade", 0.75f);
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, 0, null!);
        Expect.Sequence(new[] { Line(mesh, material, " instances=6", block: "_Fade=float (0.5)"), Line(mesh, material, " instances=6") }, Execute(buffer),
            "the block copied at the call belongs to its own draw");
        Console.WriteLine("PASS: DrawMeshInstancedIndirect copies its property block at the call and attaches it to its own draw");
    }

    private static void TheCallRefusesWhatDrawMeshRefuses()
    {
        var (mesh, material) = (Quads(), new Material(Material.SpriteLit));
        using var args = ArgsBuffer(ArgsOf(mesh, 0, 1));
        var buffer = new CommandBuffer();
        Throws<ArgumentNullException>(() => buffer.DrawMeshInstancedIndirect(null!, 0, material, 0, args, 0, null!), "a null mesh");
        Throws<ArgumentNullException>(() => buffer.DrawMeshInstancedIndirect(mesh, 0, null!, 0, args, 0, null!), "a null material");
        var gone = Quads();
        UnityEngine.Object.DestroyImmediate(gone);
        Throws<ArgumentException>(() => buffer.DrawMeshInstancedIndirect(gone, 0, material, 0, args, 0, null!), "a destroyed mesh", "destroyed");
        var goneMaterial = new Material(Material.SpriteLit);
        UnityEngine.Object.DestroyImmediate(goneMaterial);
        Throws<ArgumentException>(() => buffer.DrawMeshInstancedIndirect(mesh, 0, goneMaterial, 0, args, 0, null!), "a destroyed material", "destroyed");
        Throws<ArgumentException>(() => buffer.DrawMeshInstancedIndirect(mesh, 1, material, 0, args, 0, null!), "a sub-mesh past the last", "sub-mesh 1");
        Throws<ArgumentException>(() => buffer.DrawMeshInstancedIndirect(mesh, -1, material, 0, args, 0, null!), "a negative sub-mesh", "sub-mesh -1");
        Throws<ArgumentException>(() => buffer.DrawMeshInstancedIndirect(mesh, 0, material, -2, args, 0, null!), "a pass below -1", "pass -2");
        var holding = new MaterialPropertyBlock();
        holding.SetFloatArray("_Weights", new[] { 1f, 2f });
        Throws<NotSupportedException>(() => buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, 0, holding), "a block with an array",
            "a property block that holds an array");
        Expect.Equal(0, buffer.commandCount, "refused calls record nothing");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, 0, null!), "a released buffer", "CommandBuffer");
        Console.WriteLine("PASS: DrawMeshInstancedIndirect refuses what DrawMesh refuses when it is called");
    }

    private static void TheCallRefusesAnArgsBufferOrOffsetThatCannotHoldFiveInts()
    {
        var (mesh, material) = (Quads(), new Material(Material.SpriteLit));
        using var one = ArgsBuffer(ArgsOf(mesh, 0, 1));
        using var two = ArgsBuffer([.. ArgsOf(mesh, 0, 1), .. ArgsOf(mesh, 0, 1)]);
        var buffer = new CommandBuffer();
        Throws<ArgumentException>(() => buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, null!, 0, null!), "a null args buffer", "bufferWithArgs");
        var cases = new (ComputeBuffer Args, int Offset, string What)[]
        {
            (one, -4, "a negative offset"), (one, int.MinValue, "the smallest offset"), (one, 2, "an offset that is not a multiple of 4"),
            (one, 6, "an offset that is not a multiple of 4 inside the buffer"), (one, 4, "an offset that leaves four ints"),
            (one, 20, "an offset at the end"), (two, 24, "an offset that leaves four ints of two args"), (two, int.MaxValue - 3, "the largest aligned offset"),
        };
        foreach (var (args, offset, what) in cases)
            Throws<ArgumentException>(() => buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, offset, null!), what, "argsOffset");
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, two, 20, null!);
        Expect.Equal(1, buffer.commandCount, "only the call with room for five ints records");
        Console.WriteLine("PASS: DrawMeshInstancedIndirect refuses a null args buffer and an offset that is negative, unaligned or leaves fewer than five ints");
    }

    private static void ArgsThatDoNotDrawTheWholeSubMeshAreRefusedWhenTheBufferRuns()
    {
        var (mesh, material) = (Quads(2), new Material(Material.SpriteLit));
        var good = ArgsOf(mesh, 1, 3);
        var cases = new (string Field, int Index, int Value)[]
        {
            ("index count", 0, good[0] - 3), ("start index", 2, 0), ("base vertex", 3, 4), ("start instance", 4, 2),
        };
        foreach (var (field, index, value) in cases)
        {
            var args = ArgsBuffer(good);
            var buffer = new CommandBuffer();
            buffer.DrawMeshInstancedIndirect(mesh, 1, material, 0, args, 0, null!);
            var changed = (int[])good.Clone();
            changed[index] = value;
            args.SetData(changed);
            CommandWireReader.Take();
            var error = Expect.Catch<NotSupportedException>(() => Graphics.ExecuteCommandBuffer(buffer), $"a changed {field}");
            Ensure(error.Message.Contains(Member) && error.Message.Contains(field), $"the refusal names the member and the {field}: {error.Message}");
            Ensure(CommandWireReader.Take().Length == 0, $"a changed {field} records nothing");
            args.SetData(good);
            Expect.Sequence(new[] { Line(mesh, material, " instances=3", submesh: 1, pass: 0) }, Execute(buffer),
                $"the same buffer draws once the {field} is the sub-mesh's again");
            args.Release();
        }
        Console.WriteLine("PASS: DrawMeshInstancedIndirect refuses, naming the field, args that do not draw the whole sub-mesh from instance 0");
    }

    private static void ArgsAKernelMayHaveWrittenAreRefusedWhenTheBufferRuns()
    {
        var (mesh, material) = (Quads(), new Material(Material.SpriteLit));
        var args = ArgsBuffer(ArgsOf(mesh, 0, 8));
        var buffer = new CommandBuffer();
        buffer.DrawMeshInstancedIndirect(mesh, 0, material, 0, args, 0, null!);
        Expect.Sequence(new[] { Line(mesh, material, " instances=8") }, Execute(buffer), "before a kernel binds the buffer");
        var shader = ComputeTestSupport.Planned(ComputeTestSupport.TerrainPlan);
        shader.SetBuffer(0, "_Args", args);
        CommandWireReader.Take();
        var error = Expect.Catch<NotSupportedException>(() => Graphics.ExecuteCommandBuffer(buffer), "a buffer a kernel binds");
        Ensure(error.Message.Contains(Member) && error.Message.Contains("GPU"), $"the refusal names the member and the GPU: {error.Message}");
        Ensure(CommandWireReader.Take().Length == 0, "a refused draw records nothing");
        args.Release();
        Console.WriteLine("PASS: DrawMeshInstancedIndirect refuses args in a buffer a compute kernel is bound to, since the CPU copy may not be current");
    }
}
