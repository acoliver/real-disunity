using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>A recorded DrawMesh shows the mesh as it is when the buffer is executed: edits made since the host last stored the
/// mesh reach the host's store at that moment, with no frame flush in between, and a mesh that did not change is not stored
/// again. The host's contents are read back through the mesh store exports.</summary>
internal static class CommandBufferMeshRefreshTests
{
    internal static readonly float[] ValidTangents = [0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, -1];

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            EditsBetweenExecutionsReachTheHostWithoutAFlush();
            EditsAfterRecordingAreSeenAtExecution();
            AChannelOnlyEditReachesTheHost();
            RecalculatedTangentsReachTheHost();
            AnUnchangedMeshIsNotStoredAgain();
            AMeshBuiltBeforeItsFirstDrawIsStoredOnce();
            ManyEditsAndDrawsStoreTheMeshOnce();
            LaterEditsStillReachTheNextFlush();
            SubmittedBuffersRefreshWhenTheyRun();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    internal static Mesh Triangle(float z) => new()
    {
        vertices = Corners(z),
        triangles = [0, 1, 2],
    };

    internal static Vector3[] Corners(float z) => [new Vector3(0, 0, z), new Vector3(1, 0, z), new Vector3(0, 1, z)];

    internal static Vector4[] Tangents() => [new Vector4(0, 0, 1, 1), new Vector4(0, 0, 1, 1), new Vector4(0, 0, 1, -1)];

    internal static Vector4[] NotFiniteTangents() =>
        [new Vector4(float.NaN, 0, 1, 1), new Vector4(0, 0, 1, 1), new Vector4(0, 0, 1, -1)];

    internal static Material NewMaterial() => new(Material.SpriteLit);

    internal static CommandBuffer Drawing(Mesh mesh, Material material, int draws = 1)
    {
        var buffer = new CommandBuffer();
        for (var i = 0; i < draws; i++) buffer.DrawMesh(mesh, Matrix4x4.identity, material);
        return buffer;
    }

    /// <summary>Runs the buffer through the public entry point and returns what reached the world's offscreen list.</summary>
    internal static string[] Execute(CommandBuffer buffer)
    {
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        return CommandWireReader.Take();
    }

    internal static ulong Generation(ulong identity) => PlayerMeshStoreTests.Read(identity).Info.generation;

    internal static float StoredZ(ulong identity) => PlayerMeshStoreTests.Read(identity).Vertices[0].z;

    private static void EditsBetweenExecutionsReachTheHostWithoutAFlush()
    {
        var mesh = Triangle(0);
        var buffer = Drawing(mesh, NewMaterial());
        var first = Execute(buffer);
        var identity = mesh.RendererIdentity;
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "the first execution stores the mesh once");
        mesh.vertices = Corners(5);
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "an edit alone reaches the host only at a flush or an execution");
        var second = Execute(buffer);
        Expect.That(StoredZ(identity) == 5 && Generation(identity) == 2, $"the second execution sees the edit, got z={StoredZ(identity)} generation {Generation(identity)}");
        Expect.Sequence(first, second, "the same draw record, naming the same mesh identity");
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == 2, "the frame flush does not send a mesh the execution already stored");
        Console.WriteLine("PASS: an edit made between two executions of a buffer is in the host's store at the second, before any flush, and the flush does not send it again");
    }

    private static void EditsAfterRecordingAreSeenAtExecution()
    {
        var fresh = Triangle(1);
        var freshBuffer = Drawing(fresh, NewMaterial());
        fresh.vertices = Corners(3);
        Execute(freshBuffer);
        var freshId = fresh.RendererIdentity;
        Expect.That(StoredZ(freshId) == 3 && Generation(freshId) == 1, "a mesh without an identity is stored once, with the edit made after recording");
        Execute(freshBuffer);
        Expect.That(Generation(freshId) == 1, "executing it again stores nothing");
        var known = Triangle(1);
        var knownId = known.RendererIdentity;
        var knownBuffer = Drawing(known, NewMaterial());
        known.vertices = Corners(4);
        Execute(knownBuffer);
        Expect.That(StoredZ(knownId) == 4 && Generation(knownId) == 2, "a mesh with an identity shows the edit made after recording");
        Console.WriteLine("PASS: edits made after DrawMesh was recorded and before the buffer runs reach the host, with and without a prior identity");
    }

    private static void AChannelOnlyEditReachesTheHost()
    {
        var mesh = Triangle(0);
        mesh.normals = [Vector3.back, Vector3.back, Vector3.back];
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        Expect.That(!PlayerMeshChannelTests.Read(identity).Has(PlayerMeshChannelTests.Tangent), "the mesh has no tangents at first");
        mesh.tangents = Tangents();
        Execute(buffer);
        var stored = PlayerMeshChannelTests.Read(identity);
        Expect.Sequence(ValidTangents, stored.Values(PlayerMeshChannelTests.Tangent), "the host holds the new tangents");
        Expect.That(stored.Info.generation == 2 && Generation(identity) == 2, "the channels and the fixed mesh are stored together at generation 2");
        Expect.That(StoredZ(identity) == 0, "the positions are as they were");
        Console.WriteLine("PASS: an edit that changes only a vertex channel is stored at the next execution");
    }

    private static void RecalculatedTangentsReachTheHost()
    {
        var mesh = Triangle(0);
        mesh.normals = [Vector3.back, Vector3.back, Vector3.back];
        mesh.uv = [Vector2.zero, Vector2.right, Vector2.up];
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        Expect.That(!PlayerMeshChannelTests.Read(identity).Has(PlayerMeshChannelTests.Tangent), "the mesh has no tangents at first");
        mesh.RecalculateTangents();
        var recalculated = mesh.tangents;
        Expect.That(recalculated.Length == 3 && recalculated[0].x == 1 && recalculated.All(t => t.w is 1 or -1),
            "recalculating gives each corner a tangent along u with a w of 1 or -1");
        var wanted = recalculated.SelectMany(t => new[] { t.x, t.y, t.z, t.w }).ToArray();
        Execute(buffer);
        var stored = PlayerMeshChannelTests.Read(identity);
        Expect.Sequence(wanted, stored.Values(PlayerMeshChannelTests.Tangent), "the host holds the recalculated tangents");
        Expect.That(stored.Info.generation == 2 && Generation(identity) == 2, "the channels and the fixed mesh are stored together at generation 2");
        Console.WriteLine("PASS: RecalculateTangents on a drawn mesh reaches the host at the next execution, with the fixed mesh's generation");
    }

    private static void AnUnchangedMeshIsNotStoredAgain()
    {
        var mesh = Triangle(2);
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        Execute(buffer);
        Execute(buffer);
        Expect.That(Generation(identity) == 1, "executing a buffer three times stores an unchanged mesh once");
        mesh.vertices = Corners(6);
        Execute(buffer);
        Execute(buffer);
        Expect.That(Generation(identity) == 2 && StoredZ(identity) == 6, "an edit adds exactly one store however often the buffer runs");
        Console.WriteLine("PASS: executing a buffer again without an edit stores nothing");
    }

    private static void AMeshBuiltBeforeItsFirstDrawIsStoredOnce()
    {
        var mesh = Triangle(1);
        mesh.normals = [Vector3.back, Vector3.back, Vector3.back];
        mesh.uv = [Vector2.zero, Vector2.right, Vector2.up];
        mesh.tangents = Tangents();
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == 1 && PlayerMeshChannelTests.Read(identity).Info.generation == 1,
            "the frame flush right after the first store sends nothing for a mesh built before it had an identity");
        Execute(buffer);
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == 1, "neither does a second execution and flush");
        Console.WriteLine("PASS: a mesh built before its first draw is stored once, and the flush right after sends nothing");
    }

    private static void ManyEditsAndDrawsStoreTheMeshOnce()
    {
        var mesh = Triangle(0);
        var material = NewMaterial();
        var wide = Drawing(mesh, material, 3);
        var other = Drawing(mesh, material);
        Execute(wide);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(1);
        mesh.uv = [Vector2.zero, Vector2.right, Vector2.up];
        mesh.triangles = [0, 2, 1];
        var records = Execute(wide);
        Execute(other);
        Expect.That(records.Length == 3, $"three draws are recorded, got {records.Length}");
        Expect.That(Generation(identity) == 2, $"three edits and four draws of one mesh store it once, got generation {Generation(identity)}");
        var stored = PlayerMeshStoreTests.Read(identity);
        Expect.That(stored.Vertices[0].z == 1 && stored.Vertices[1].u == 1, "the single store holds every edit");
        Expect.Sequence(new uint[] { 0, 2, 1 }, stored.Indices, "including the new triangle order");
        Console.WriteLine("PASS: several edits and several draws of one mesh between executions store it once");
    }

    private static void LaterEditsStillReachTheNextFlush()
    {
        var mesh = Triangle(0);
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(7);
        Execute(buffer);
        mesh.vertices = Corners(8);
        Expect.That(StoredZ(identity) == 7, "an edit after the execution waits for the flush");
        DisunityHost.StoreChangedMeshes();
        Expect.That(StoredZ(identity) == 8 && Generation(identity) == 3, "the flush stores an edit made after the execution");
        Execute(buffer);
        Expect.That(Generation(identity) == 3, "the following execution stores nothing");
        Console.WriteLine("PASS: an edit made after an execution is still stored by the frame flush, once");
    }

    private static void SubmittedBuffersRefreshWhenTheyRun()
    {
        var mesh = Triangle(0);
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        CommandWireReader.Take();
        new ScriptableRenderContext().ExecuteCommandBuffer(buffer);
        mesh.vertices = Corners(9);
        Expect.That(StoredZ(identity) == 0, "queueing the buffer on a context stores nothing");
        new ScriptableRenderContext().Submit();
        var records = CommandWireReader.Take();
        Expect.That(records.Length == 1 && records[0].StartsWith($"DrawMesh mesh={identity} "), "the submitted draw names the same mesh");
        Expect.That(StoredZ(identity) == 9 && Generation(identity) == 2, "Submit runs the draw, which stores the edit made after the buffer was queued");
        Console.WriteLine("PASS: a buffer submitted through a context stores the mesh's edits when it runs");
    }
}
