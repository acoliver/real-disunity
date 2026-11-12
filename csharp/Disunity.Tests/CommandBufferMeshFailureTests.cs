using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static CommandBufferMeshLifecycleTests;
using static CommandBufferMeshRefreshTests;

/// <summary>What the stored mesh and its callers do when the host refuses a store, or a draw outlives a sub-mesh. The host's two
/// mesh stores are not one transaction (the fixed mesh can be stored and its channels refused), so these tests assert what
/// callers can rely on: a mesh whose store failed is refused at every draw until it is corrected, work queued behind a
/// failure is not lost, and a filter or a buffer does not publish anything for an assignment or a draw it refuses.</summary>
internal static class CommandBufferMeshFailureTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            AFrameFlushTheHostRefusesLeavesTheDrawRefusedUntilTheMeshIsCorrected();
            ARefusedFrameFlushIsNotRepeatedByTheNextFlushAlone();
            QueuedMeshesSurviveARefusedFlushAndEachRefusedMeshIsTriedOnce();
            AFilterKeepsItsMeshWhenTheReplacementIsRefused();
            AnEditedMeshAssignedToAFilterIsStoredOnce();
            AnInvalidFilterDoesNotPublishTheMeshAssignedToIt();
            AMeshKeptAcrossWorldsIsStoredWhenAFilterTakesIt();
            ASubMeshTheMeshLostIsRefusedWhenTheBufferRuns();
            ASubMeshTheMeshLostIsRefusedWhenAContextSubmitsTheBuffer();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static MeshFilter NewFilter() => new GameObject("mesh object").AddComponent<MeshFilter>();

    /// <summary>A mesh with channels that a buffer drew once, so the world holds it at generation 1 and nothing is queued.</summary>
    private static (Mesh Mesh, CommandBuffer Buffer, ulong Identity) DrawnOnce()
    {
        var mesh = TriangleWithChannels(0, Tangents());
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        return (mesh, buffer, mesh.RendererIdentity);
    }

    /// <summary>An edit of the positions together with tangents the host refuses.</summary>
    private static void EditRefusedByTheHost(Mesh mesh, float z)
    {
        mesh.vertices = Corners(z);
        mesh.tangents = NotFiniteTangents();
    }

    private static void RefusedWhatever(Action action, string what)
    {
        try { action(); }
        catch (Exception) { return; }
        throw new InvalidOperationException($"{what}: nothing was thrown");
    }

    private static void AFrameFlushTheHostRefusesLeavesTheDrawRefusedUntilTheMeshIsCorrected()
    {
        var (mesh, buffer, identity) = DrawnOnce();
        EditRefusedByTheHost(mesh, 4);
        Expect.Throws<DisunityException>(DisunityHost.StoreChangedMeshes, "the frame flush of a mesh whose tangents the host refuses");
        Refused<DisunityException>(buffer, "the unchanged mesh executed after the refused flush");
        Refused<DisunityException>(buffer, "the same unchanged mesh executed again");
        mesh.tangents = Tangents();
        var records = Execute(buffer);
        ExpectDrawnWithValidTangents(identity, records, "the corrected mesh");
        Expect.That(mesh.RendererIdentity == identity, "the correction keeps the mesh's identity");
        Expect.That(StoredZ(identity) == 4, "the position edit made with the refused one is stored with the correction");
        var generation = Generation(identity);
        Execute(buffer);
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == generation, "once stored again, neither another execution nor the flush stores it");
        Console.WriteLine("PASS: a mesh whose frame flush the host refused is refused at every draw until it is corrected, then drawn under the same identity");
    }

    private static void ARefusedFrameFlushIsNotRepeatedByTheNextFlushAlone()
    {
        var (mesh, _, identity) = DrawnOnce();
        EditRefusedByTheHost(mesh, 4);
        Expect.Throws<DisunityException>(DisunityHost.StoreChangedMeshes, "the frame flush of a mesh whose tangents the host refuses");
        var attempted = Generation(identity);
        DisunityHost.StoreChangedMeshes();
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == attempted, "later frames do not try the refused mesh again without an edit or a draw");
        Console.WriteLine("PASS: a refused frame flush is not repeated by the next flushes when nothing was edited or drawn");
    }

    private static void QueuedMeshesSurviveARefusedFlushAndEachRefusedMeshIsTriedOnce()
    {
        var good = new[] { DrawnOnce(), DrawnOnce() };
        var refused = new[] { DrawnOnce(), DrawnOnce() };
        foreach (var (mesh, _, _) in good) mesh.vertices = Corners(6);
        foreach (var (mesh, _, _) in refused) EditRefusedByTheHost(mesh, 6);
        var failures = new bool[4];
        for (var flush = 0; flush < failures.Length; flush++)
        {
            try { DisunityHost.StoreChangedMeshes(); }
            catch (DisunityException) { failures[flush] = true; }
        }
        Expect.That(failures.Count(failed => failed) == 2, $"the two refused meshes fail one flush each, whatever their order, got [{string.Join(", ", failures)}]");
        Expect.That(!failures[2] && !failures[3], $"the queue is quiet after the second failure, got [{string.Join(", ", failures)}]");
        foreach (var (_, _, identity) in good)
            Expect.That(StoredZ(identity) == 6 && Generation(identity) == 2, "a valid mesh queued beside refused ones is stored");
        foreach (var (_, _, identity) in refused)
            Expect.That(Generation(identity) == 2, $"a refused mesh was tried once, got generation {Generation(identity)}");
        DisunityHost.StoreChangedMeshes();
        Expect.That(good.Concat(refused).All(entry => Generation(entry.Identity) == 2), "a further flush stores nothing");
        Console.WriteLine("PASS: meshes queued behind a refused frame flush are kept and each refused mesh is tried once, in any order");
    }

    private static void AFilterKeepsItsMeshWhenTheReplacementIsRefused()
    {
        var old = Triangle(0);
        var filter = NewFilter();
        filter.sharedMesh = old;
        var oldIdentity = old.RendererIdentity;
        var replacement = TriangleWithChannels(0, Tangents());
        var replacementIdentity = replacement.RendererIdentity;
        replacement.tangents = NotFiniteTangents();
        Expect.Throws<DisunityException>(() => filter.sharedMesh = replacement, "a replacement mesh with edits the host refuses");
        Expect.That(ReferenceEquals(filter.sharedMesh, old), "the filter still names its old mesh");
        Expect.That(PlayerMeshStoreTests.ReadDrawn(filter) == oldIdentity, "and still draws it");
        replacement.tangents = Tangents();
        filter.sharedMesh = replacement;
        Expect.That(ReferenceEquals(filter.sharedMesh, replacement), "the corrected mesh is taken when the assignment is repeated");
        Expect.That(PlayerMeshStoreTests.ReadDrawn(filter) == replacementIdentity, "and drawn under its identity");
        var stored = PlayerMeshChannelTests.Read(replacementIdentity);
        Expect.Sequence(ValidTangents, stored.Values(PlayerMeshChannelTests.Tangent), "the host holds the corrected tangents");
        Expect.That(stored.Info.generation == Generation(replacementIdentity), "the channels carry the generation of the fixed mesh");
        Console.WriteLine("PASS: a filter keeps its mesh and its drawn identity when the replacement is refused, and takes the corrected one on a retry");
    }

    private static void AnEditedMeshAssignedToAFilterIsStoredOnce()
    {
        var mesh = Triangle(0);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(6);
        var filter = NewFilter();
        filter.sharedMesh = mesh;
        Expect.That(StoredZ(identity) == 6 && Generation(identity) == 2, "assigning an edited mesh stores the edit");
        Expect.That(PlayerMeshStoreTests.ReadDrawn(filter) == identity, "and the filter draws it");
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == 2, "the frame flush has nothing more to send for it");
        Console.WriteLine("PASS: a mesh edited since its last store is stored once when a filter takes it, and not again by the flush");
    }

    private static void AnInvalidFilterDoesNotPublishTheMeshAssignedToIt()
    {
        var filter = NewFilter();
        var kept = Triangle(0);
        var keptIdentity = kept.RendererIdentity;
        ReloadWorld();
        var fresh = Triangle(0);
        var next = DisunityHost.AllocatePlayerMeshIdentity() + 1;
        RefusedWhatever(() => filter.sharedMesh = fresh, "assigning a mesh to a filter of a world that was unloaded");
        Expect.That(DisunityHost.AllocatePlayerMeshIdentity() == next, "a refused assignment gives a fresh mesh no identity");
        RefusedWhatever(() => filter.sharedMesh = kept, "assigning a kept mesh to a filter of a world that was unloaded");
        Expect.That(PlayerMeshStoreTests.Read(keptIdentity).Info.present == 0, "and does not store a kept mesh in the world loaded now");
        Console.WriteLine("PASS: a filter the world does not hold refuses an assignment before the mesh is given an identity or stored");
    }

    private static void AMeshKeptAcrossWorldsIsStoredWhenAFilterTakesIt()
    {
        var kept = Triangle(0);
        var keptIdentity = kept.RendererIdentity;
        ReloadWorld();
        Expect.That(PlayerMeshStoreTests.Read(keptIdentity).Info.present == 0, "the world loaded now holds nothing of the mesh");
        var filter = NewFilter();
        filter.sharedMesh = kept;
        var info = PlayerMeshStoreTests.Read(keptIdentity).Info;
        Expect.That(info.present == 1 && info.generation == 1, "a filter taking the kept mesh stores it in the world loaded now, once");
        Expect.That(PlayerMeshStoreTests.ReadDrawn(filter) == keptIdentity, "and draws it under the identity it had");
        Console.WriteLine("PASS: a mesh kept across an unloaded world is stored in the world loaded next when a filter takes it");
    }

    private static CommandBuffer DrawingSubMeshOne(Mesh mesh)
    {
        var buffer = new CommandBuffer();
        buffer.DrawMesh(mesh, Matrix4x4.identity, NewMaterial(), 1);
        return buffer;
    }

    private static void ASubMeshTheMeshLostIsRefusedWhenTheBufferRuns()
    {
        var mesh = Triangle(0);
        mesh.subMeshCount = 2;
        var buffer = DrawingSubMeshOne(mesh);
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(5);
        mesh.subMeshCount = 1;
        Refused<ArgumentException>(buffer, "a sub-mesh the mesh lost after recording");
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "the refused draw did not store the mesh's edits");
        mesh.subMeshCount = 2;
        var records = Execute(buffer);
        Expect.That(records.Length == 1 && records[0].StartsWith($"DrawMesh mesh={identity} "), "once the sub-mesh is back the draw names the mesh");
        Expect.That(StoredZ(identity) == 5 && Generation(identity) == 2, "and stores the pending edits once");
        var fresh = Triangle(0);
        fresh.subMeshCount = 2;
        var freshBuffer = DrawingSubMeshOne(fresh);
        fresh.subMeshCount = 1;
        var next = DisunityHost.AllocatePlayerMeshIdentity() + 1;
        Refused<ArgumentException>(freshBuffer, "a sub-mesh a fresh mesh lost after recording");
        Expect.That(DisunityHost.AllocatePlayerMeshIdentity() == next, "the refused draw gives a fresh mesh no identity");
        Console.WriteLine("PASS: a sub-mesh the mesh lost after DrawMesh was recorded is refused when the buffer runs, with nothing stored for the draw");
    }

    private static void ASubMeshTheMeshLostIsRefusedWhenAContextSubmitsTheBuffer()
    {
        var mesh = Triangle(0);
        mesh.subMeshCount = 2;
        var buffer = DrawingSubMeshOne(mesh);
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(5);
        CommandWireReader.Take();
        new ScriptableRenderContext().ExecuteCommandBuffer(buffer);
        mesh.subMeshCount = 1;
        Expect.Throws<ArgumentException>(() => new ScriptableRenderContext().Submit(), "a sub-mesh lost after its buffer was queued on a context");
        Expect.That(CommandWireReader.Take().Length == 0, "the refused draw records nothing");
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "the refused draw did not store the mesh's edits");
        mesh.subMeshCount = 2;
        new ScriptableRenderContext().ExecuteCommandBuffer(buffer);
        new ScriptableRenderContext().Submit();
        var records = CommandWireReader.Take();
        Expect.That(records.Length == 1 && records[0].StartsWith($"DrawMesh mesh={identity} "), "once the sub-mesh is back Submit draws the mesh");
        Expect.That(StoredZ(identity) == 5 && Generation(identity) == 2, "and stores the pending edits once");
        Console.WriteLine("PASS: a sub-mesh lost after its buffer was queued on a context is refused at Submit, with nothing stored");
    }
}
