using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>One command buffer interleaves typed global vector and matrix arrays with two recorded DrawMesh calls, so the
/// array records and the mesh refresh at execution have to work together. The world's ordered work is read back with the
/// command wire reader, and the host's stored meshes, their channels and generations are read through the mesh store exports.
/// The expected text and numbers are written out here: the matrix columns, the vector components and the draw matrices are
/// spelled in the order the world holds them, not derived with the engine's own conversion.</summary>
internal static unsafe class CommandBufferMeshArrayTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ExecuteRefreshesBothDrawnMeshesOnceBetweenArrayRecords();
            SubmitRefreshesBothDrawnMeshesOnceBetweenArrayRecords();
            ArraysMeshesAndMaterialsAreRestoredTogetherInANewWorld();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    // The ordered records the buffer sends, as the command wire reader describes them. A matrix element (row, column) is
    // base + 10 * row + column, and the world holds a matrix column by column.
    private const string VectorsBeforeA = "SetGlobal _MaVec=vectors [3] (1 2 3 4) (5 6 7 8) (-9 -10 -11 -12)";
    private const string MatricesBeforeA = "SetGlobal _MaMat=matrices [2] (100 110 120 130 101 111 121 131 102 112 122 132 103 113 123 133) (300 310 320 330 301 311 321 331 302 312 322 332 303 313 323 333)";
    private const string VectorsBeforeB = "SetGlobal _MaVec=vectors [2] (0.5 -1 2 -3) (7 0 0 1)";
    private const string MatricesBeforeB = "SetGlobal _MaMat=matrices [1] (500 510 520 530 501 511 521 531 502 512 522 532 503 513 523 533)";

    // Draw matrices with scale, shear and translation make transposition change the checked shear and translation entries.
    private const string ColumnsOfDrawMatrixA = "(2 0 0 0 5 3 0 0 0 0 4 0 7 8 9 1)";
    private const string ColumnsOfDrawMatrixB = "(1 6 0 0 0 2 0 0 0 0 1 0 -1 0.5 3 1)";

    // Static arrays, which go to the world's global state and record no work. Matrix base 700.
    private static readonly float[] StaticVectors = [11, 12, 13, 14, 15, 16, 17, 18];
    private static readonly float[] StaticMatrices = [700, 710, 720, 730, 701, 711, 721, 731, 702, 712, 722, 732, 703, 713, 723, 733];
    private static readonly float[] LateVectors = [21, 22, 23, 24];

    // Tangents, three vertices of four floats each, w of 1 or -1.
    private static readonly float[] TangentsA0 = [1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, -1];
    private static readonly float[] TangentsA1 = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, -1];
    private static readonly float[] TangentsA2 = [0, 0, 1, 1, 0, 0, 1, -1, 0, 0, 1, 1];
    private static readonly float[] TangentsB0 = [0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, -1];
    private static readonly float[] TangentsB1 = [1, 0, 0, -1, 0, 1, 0, 1, 0, 0, 1, 1];

    /// <summary>What a stored mesh holds: its positions (three floats a vertex) and its tangents (four floats a vertex).</summary>
    private readonly record struct Content(float[] Positions, float[] Tangents);

    /// <summary>The triangle at x..x+1 and z: positions in vertex order.</summary>
    private static float[] Flat(float x, float z) => [x, 0, z, x + 1, 0, z, x, 1, z];

    private static Vector3[] Corners(float x, float z) => [new Vector3(x, 0, z), new Vector3(x + 1, 0, z), new Vector3(x, 1, z)];

    private static Vector4[] ToVectors(float[] floats) =>
        Enumerable.Range(0, floats.Length / 4).Select(i => new Vector4(floats[i * 4], floats[i * 4 + 1], floats[i * 4 + 2], floats[i * 4 + 3])).ToArray();

    private static Mesh Shaped(float x, float z, float[] tangents)
    {
        var mesh = new Mesh { vertices = Corners(x, z), triangles = [0, 1, 2] };
        mesh.normals = [Vector3.back, Vector3.back, Vector3.back];
        mesh.tangents = ToVectors(tangents);
        return mesh;
    }

    /// <summary>A matrix whose listed elements are set and the rest are zero.</summary>
    private static Matrix4x4 WithCells(params (int Row, int Column, float Value)[] cells)
    {
        var matrix = new Matrix4x4();
        foreach (var (row, column, value) in cells) matrix[row, column] = value;
        return matrix;
    }

    private static Matrix4x4 Numbered(float baseValue) =>
        WithCells(Enumerable.Range(0, 16).Select(i => (i / 4, i % 4, baseValue + 10 * (i / 4) + i % 4)).ToArray());

    private static Material QueuedAt(int queue)
    {
        var material = CommandBufferMeshRefreshTests.NewMaterial();
        material.renderQueue = queue;
        return material;
    }

    /// <summary>Two meshes, two materials and one buffer: vectors and matrices, a draw of mesh A, other vectors and matrices
    /// under the same names, a draw of mesh B. Three static arrays are set too. Disposing removes them.</summary>
    private sealed class Scene : IDisposable
    {
        public Mesh A { get; } = Shaped(0, 0, TangentsA0);
        public Mesh B { get; } = Shaped(2, 10, TangentsB0);
        public Material MaterialA { get; } = QueuedAt(2001);
        public Material MaterialB { get; } = QueuedAt(2002);
        public CommandBuffer Buffer { get; } = new();
        public ulong IdA { get; private set; }
        public ulong IdB { get; private set; }

        public Scene()
        {
            Buffer.SetGlobalVectorArray("_MaVec", ToVectors([1, 2, 3, 4, 5, 6, 7, 8, -9, -10, -11, -12]));
            Buffer.SetGlobalMatrixArray("_MaMat", new[] { Numbered(100), Numbered(300) });
            Buffer.DrawMesh(A, WithCells((0, 0, 2), (0, 1, 5), (1, 1, 3), (2, 2, 4), (0, 3, 7), (1, 3, 8), (2, 3, 9), (3, 3, 1)), MaterialA);
            Buffer.SetGlobalVectorArray(Shader.PropertyToID("_MaVec"), new List<Vector4>(ToVectors([0.5f, -1, 2, -3, 7, 0, 0, 1])));
            Buffer.SetGlobalMatrixArray(Shader.PropertyToID("_MaMat"), new List<Matrix4x4> { Numbered(500) });
            Buffer.DrawMesh(B, WithCells((0, 0, 1), (1, 0, 6), (1, 1, 2), (2, 2, 1), (0, 3, -1), (1, 3, 0.5f), (2, 3, 3), (3, 3, 1)), MaterialB);
            Shader.SetGlobalVectorArray("_MaStaticVec", ToVectors(StaticVectors));
            Shader.SetGlobalMatrixArray("_MaStaticMat", new[] { Numbered(700) });
        }

        /// <summary>Reads the identities after the first execution, which is what stored the meshes.</summary>
        public void Bind() => (IdA, IdB) = (A.RendererIdentity, B.RendererIdentity);

        /// <summary>The six records one execution sends: the draws name the meshes' identities and the materials' ids in the
        /// loaded world, which exist once the buffer has run.</summary>
        public string[] Expected() =>
        [
            VectorsBeforeA, MatricesBeforeA,
            $"DrawMesh mesh={IdA} matrix={ColumnsOfDrawMatrixA} material={MaterialA.WorldLink.Id} submesh=0 pass=-1 block=[-]",
            VectorsBeforeB, MatricesBeforeB,
            $"DrawMesh mesh={IdB} matrix={ColumnsOfDrawMatrixB} material={MaterialB.WorldLink.Id} submesh=0 pass=-1 block=[-]",
        ];

        public void Dispose()
        {
            foreach (var name in new[] { "_MaStaticVec", "_MaLateVec" }) Shader.Globals.VectorArrays.Remove(Shader.PropertyToID(name));
            Shader.Globals.MatrixArrays.Remove(Shader.PropertyToID("_MaStaticMat"));
            Buffer.Release();
        }
    }

    /// <summary>Runs the buffer through the public entry point and returns the records the world received, in order.</summary>
    private static string[] Execute(Scene scene) => CommandBufferMeshRefreshTests.Execute(scene.Buffer);

    private static void ExpectNoWork(string when) =>
        Expect.That(CommandWireReader.Take().Length == 0, $"{when}: the world received work");

    /// <summary>The host's stored positions, tangents and generations of one mesh. The fixed mesh and its channels are stored
    /// together, so they carry one generation.</summary>
    private static void ExpectStored(string what, ulong identity, ulong generation, Content content)
    {
        var fixedMesh = PlayerMeshStoreTests.Read(identity);
        var channels = PlayerMeshChannelTests.Read(identity);
        Expect.That(fixedMesh.Info.present == 1, $"{what}: the host holds no mesh under identity {identity}");
        Expect.That(fixedMesh.Info.generation == generation && channels.Info.generation == generation,
            $"{what}: expected generation {generation}, the fixed mesh is at {fixedMesh.Info.generation} and its channels at {channels.Info.generation}");
        var positions = fixedMesh.Vertices.SelectMany(v => new[] { v.x, v.y, v.z });
        Expect.Sequence(content.Positions, positions, $"{what}: the host's positions");
        Expect.Sequence(content.Tangents, channels.Values(PlayerMeshChannelTests.Tangent), $"{what}: the host's tangents");
    }

    private static void ExpectBoth(Scene scene, string when, ulong generation, Content a, Content b)
    {
        ExpectStored($"mesh A {when}", scene.IdA, generation, a);
        ExpectStored($"mesh B {when}", scene.IdB, generation, b);
    }

    private static void ExecuteRefreshesBothDrawnMeshesOnceBetweenArrayRecords()
    {
        CommandWireReader.Take();
        using var scene = new Scene();
        ExpectNoWork("recording the arrays and draws and setting static arrays");
        var first = Execute(scene);
        scene.Bind();
        Expect.That(scene.IdA != scene.IdB, "the two meshes have different identities");
        Expect.Sequence(scene.Expected(), first, "the first execution sends the arrays and draws in recorded order");
        var (a0, b0) = (new Content(Flat(0, 0), TangentsA0), new Content(Flat(2, 10), TangentsB0));
        ExpectBoth(scene, "after the first execution", 1, a0, b0);

        scene.A.vertices = Corners(0, 5);
        scene.B.tangents = ToVectors(TangentsB1);
        ExpectBoth(scene, "after its edit, before the buffer runs again", 1, a0, b0);
        ExpectNoWork("editing the meshes");
        var second = Execute(scene);
        Expect.Sequence(first, second, "the second execution sends the same records, naming the same meshes");
        var (a1, b1) = (new Content(Flat(0, 5), TangentsA0), new Content(Flat(2, 10), TangentsB1));
        ExpectBoth(scene, "after the second execution, before any frame flush", 2, a1, b1);

        DisunityHost.StoreChangedMeshes();
        ExpectBoth(scene, "after the frame flush that follows the execution", 2, a1, b1);
        Expect.Sequence(first, Execute(scene), "a third execution sends the same records");
        ExpectBoth(scene, "after a third execution", 2, a1, b1);
        Console.WriteLine("PASS: executing a buffer that interleaves typed arrays and two DrawMesh records stores each edited mesh once with the arrays in order, and the flush that follows stores nothing");
    }

    private static void SubmitRefreshesBothDrawnMeshesOnceBetweenArrayRecords()
    {
        CommandWireReader.Take();
        ScriptableRenderContext.DiscardPending();
        using var scene = new Scene();
        var first = Execute(scene);
        scene.Bind();
        var (a0, b0) = (new Content(Flat(0, 0), TangentsA0), new Content(Flat(2, 10), TangentsB0));
        ExpectBoth(scene, "after the first execution", 1, a0, b0);

        scene.A.vertices = Corners(0, 6);
        scene.A.tangents = ToVectors(TangentsA1);
        new ScriptableRenderContext().ExecuteCommandBuffer(scene.Buffer);
        Expect.That(ScriptableRenderContext.PendingCount == 6 && scene.Buffer.commandCount == 6, "the context holds the six commands until Submit");
        ExpectNoWork("queueing the buffer on a context");
        ExpectBoth(scene, "after queueing the buffer", 1, a0, b0);

        scene.A.vertices = Corners(0, 7);
        scene.B.vertices = Corners(2, 11);
        new ScriptableRenderContext().Submit();
        var submitted = CommandWireReader.Take();
        Expect.Sequence(first, submitted, "Submit sends the same records, naming the same meshes");
        Expect.Sequence(scene.Expected(), submitted, "Submit preserves literal array order and column-major draw matrices");
        var (a1, b1) = (new Content(Flat(0, 7), TangentsA1), new Content(Flat(2, 11), TangentsB0));
        ExpectBoth(scene, "after Submit, before any frame flush", 2, a1, b1);

        DisunityHost.StoreChangedMeshes();
        ExpectBoth(scene, "after the frame flush that follows Submit", 2, a1, b1);
        Console.WriteLine("PASS: submitting a context that holds that buffer stores both meshes with every edit made before Submit once, and the flush that follows stores nothing");
    }

    private static byte[] Entry(string name, float[] floats, int width)
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        var bytes = Encoding.UTF8.GetBytes(name);
        writer.Write((uint)bytes.Length);
        writer.Write(bytes);
        writer.Write((uint)(floats.Length / width));
        foreach (var value in floats) writer.Write(value);
        writer.Flush();
        return stream.ToArray();
    }

    /// <summary>The native snapshot of the frame after recording a keyword so the frame has work. It drains the frame's work.</summary>
    private static byte[] Snapshot()
    {
        var buffer = new CommandBuffer();
        buffer.EnableShaderKeyword("_MA_TRIGGER");
        Graphics.ExecuteCommandBuffer(buffer);
        buffer.Release();
        var world = DisunityHost.WorldHandle;
        uint length;
        DisunityHost.ThrowIfError(NativeBindings.disunity_graphics_snapshot(world, null, 0, &length));
        var bytes = new byte[length];
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_graphics_snapshot(world, pointer, length, &length));
        return bytes;
    }

    private static int Count(byte[] haystack, byte[] needle)
    {
        var count = 0;
        for (var at = 0; at + needle.Length <= haystack.Length; at++)
            if (haystack.AsSpan(at, needle.Length).SequenceEqual(needle)) count++;
        return count;
    }

    /// <summary>The world's initial global state holds each static array once. Before any ordered work has run in the world
    /// it holds none of the buffer's arrays either; once it has run, nothing is asserted about those names.</summary>
    private static void ExpectInitialGlobals(string when, bool late, bool orderedWorkHasRun = false)
    {
        var snapshot = Snapshot();
        Expect.That(Count(snapshot, Entry("_MaStaticVec", StaticVectors, 4)) == 1, $"{when}: the static vectors are in the global state once");
        Expect.That(Count(snapshot, Entry("_MaStaticMat", StaticMatrices, 16)) == 1, $"{when}: the static matrices are in the global state once");
        Expect.That(Count(snapshot, Entry("_MaLateVec", LateVectors, 4)) == (late ? 1 : 0), $"{when}: the vectors set while no world was loaded are {(late ? "" : "not ")}in the global state");
        if (orderedWorkHasRun) return;
        Expect.That(Count(snapshot, Encoding.UTF8.GetBytes("_MaVec")) == 0 && Count(snapshot, Encoding.UTF8.GetBytes("_MaMat")) == 0,
            $"{when}: no array of the buffer is in the world before it runs");
    }

    private static ulong MaterialOf(string drawRecord)
    {
        var from = drawRecord.IndexOf("material=", StringComparison.Ordinal) + "material=".Length;
        return ulong.Parse(drawRecord[from..drawRecord.IndexOf(' ', from)]);
    }

    private static DisunityMaterialInfo MaterialInfo(ulong material)
    {
        DisunityMaterialInfo info;
        DisunityHost.ThrowIfError(NativeBindings.disunity_material_read(DisunityHost.WorldHandle, material, &info));
        return info;
    }

    /// <summary>The two draws name materials the loaded world holds, with the shader and render queue the managed materials have.</summary>
    private static void ExpectMaterialsBound(Scene scene, string[] records)
    {
        var (idA, idB) = (MaterialOf(records[2]), MaterialOf(records[5]));
        var shader = scene.MaterialA.shader.WorldLink.Id;
        Expect.That(idA != idB && scene.MaterialA.WorldLink.IsCurrent && scene.MaterialB.WorldLink.IsCurrent, "the two materials are two resources of the loaded world");
        var (infoA, infoB) = (MaterialInfo(idA), MaterialInfo(idB));
        Expect.That(infoA.render_queue == 2001 && infoB.render_queue == 2002, $"the world's materials carry the render queues 2001 and 2002, got {infoA.render_queue} and {infoB.render_queue}");
        Expect.That(infoA.shader == shader && infoB.shader == shader, "the world's materials use the shader of the managed materials");
    }

    private static void ArraysMeshesAndMaterialsAreRestoredTogetherInANewWorld()
    {
        CommandWireReader.Take();
        ScriptableRenderContext.DiscardPending();
        using var scene = new Scene();
        ExpectInitialGlobals("before the world is replaced", late: false);
        var first = Execute(scene);
        scene.Bind();
        Expect.Sequence(scene.Expected(), first, "the first execution sends the arrays and draws in recorded order");
        scene.A.vertices = Corners(0, 8);
        scene.B.tangents = ToVectors(TangentsB1);

        var before = DisunityHost.WorldHandle;
        DisunityHost.UnloadWorld();
        Shader.SetGlobalVectorArray("_MaLateVec", ToVectors(LateVectors));
        DisunityHost.LoadWorld();
        Expect.That(DisunityHost.WorldHandle != before, "the world loaded next is a different world");
        Expect.That(PlayerMeshStoreTests.Read(scene.IdA).Info.present == 0 && PlayerMeshStoreTests.Read(scene.IdB).Info.present == 0, "the new world holds neither mesh yet");
        ExpectInitialGlobals("in the new world", late: true);
        CommandWireReader.Take();

        var reloaded = Execute(scene);
        Expect.Sequence(scene.Expected(), reloaded, "the surviving buffer sends the same arrays and draws, naming the same meshes, in the new world");
        ExpectMaterialsBound(scene, reloaded);
        var (a1, b1) = (new Content(Flat(0, 8), TangentsA0), new Content(Flat(2, 10), TangentsB1));
        ExpectBoth(scene, "in the new world after the first execution", 1, a1, b1);
        DisunityHost.StoreChangedMeshes();
        ExpectBoth(scene, "in the new world after the frame flush", 1, a1, b1);

        scene.A.vertices = Corners(0, 9);
        scene.A.tangents = ToVectors(TangentsA2);
        new ScriptableRenderContext().ExecuteCommandBuffer(scene.Buffer);
        scene.B.vertices = Corners(2, 11);
        scene.B.tangents = ToVectors(TangentsB0);
        ExpectNoWork("queueing and editing after queueing the buffer in the new world");
        new ScriptableRenderContext().Submit();
        Expect.Sequence(reloaded, CommandWireReader.Take(), "Submit in the new world sends the same records");
        var (a2, b2) = (new Content(Flat(0, 9), TangentsA2), new Content(Flat(2, 11), TangentsB0));
        ExpectBoth(scene, "in the new world after Submit", 2, a2, b2);
        DisunityHost.StoreChangedMeshes();
        ExpectBoth(scene, "in the new world after the flush that follows Submit", 2, a2, b2);
        ExpectInitialGlobals("after the draws in the new world", late: true, orderedWorkHasRun: true);
        Console.WriteLine("PASS: after the world is unloaded and loaded the static arrays are in the new world's global state, the surviving buffer's arrays and draws are sent again, and the same meshes and materials are stored in it once");
    }
}
