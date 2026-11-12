using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static CommandBufferMeshRefreshTests;

/// <summary>When a recorded DrawMesh runs, a mesh the host refuses stays refused until it is corrected, a mesh the loaded world
/// does not hold is stored whatever world it was stored in before, and a mesh or material destroyed since recording is
/// refused before anything is stored or made for it. The host's contents are read back through the mesh store exports.</summary>
internal static class CommandBufferMeshLifecycleTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            AFreshMeshTheHostRefusesStaysRefusedUntilItIsCorrected();
            AnEditTheHostRefusesStaysRefusedUntilItIsCorrected();
            ADestroyedMeshOrMaterialIsRefusedWhenTheBufferRuns();
            ADestroyedMeshIsRefusedWhenAContextSubmitsTheBuffer();
            AMeshFirstExecutedWithNoWorldIsStoredOnceOneIsLoaded();
            AMeshKeptAcrossWorldsIsStoredInTheNewWorld();
            AnEditPendingWhenTheWorldIsUnloadedIsStoredInTheNewWorld();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    /// <summary>Executes the buffer, which must throw <typeparamref name="TException"/> and leave no draw in the world.</summary>
    internal static void Refused<TException>(CommandBuffer buffer, string what) where TException : Exception
    {
        CommandWireReader.Take();
        Expect.Throws<TException>(() => Graphics.ExecuteCommandBuffer(buffer), what);
        Expect.That(CommandWireReader.Take().Length == 0, $"{what}: a refused draw records nothing");
    }

    internal static Mesh TriangleWithChannels(float z, Vector4[] tangents)
    {
        var mesh = Triangle(z);
        mesh.normals = [Vector3.back, Vector3.back, Vector3.back];
        mesh.tangents = tangents;
        return mesh;
    }

    internal static void ExpectDrawnWithValidTangents(ulong identity, string[] records, string what)
    {
        Expect.That(records.Length == 1 && records[0].StartsWith($"DrawMesh mesh={identity} "), $"{what}: the draw names the mesh");
        var stored = PlayerMeshChannelTests.Read(identity);
        Expect.Sequence(ValidTangents, stored.Values(PlayerMeshChannelTests.Tangent), $"{what}: the host holds the tangents");
        Expect.That(stored.Info.generation == Generation(identity), $"{what}: the channels carry the generation of the fixed mesh");
    }

    private static void AFreshMeshTheHostRefusesStaysRefusedUntilItIsCorrected()
    {
        var mesh = TriangleWithChannels(0, NotFiniteTangents());
        var buffer = Drawing(mesh, NewMaterial());
        Refused<DisunityException>(buffer, "a mesh whose tangents are not finite, executed for the first time");
        Refused<DisunityException>(buffer, "the same unchanged mesh executed again");
        mesh.tangents = Tangents();
        var records = Execute(buffer);
        var identity = mesh.RendererIdentity;
        ExpectDrawnWithValidTangents(identity, records, "the corrected mesh");
        var generation = Generation(identity);
        Execute(buffer);
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == generation, "once stored, neither another execution nor the flush stores it again");
        Console.WriteLine("PASS: a fresh mesh the host refuses is refused at every execution until an edit corrects it, then drawn and stored once");
    }

    private static void AnEditTheHostRefusesStaysRefusedUntilItIsCorrected()
    {
        var mesh = TriangleWithChannels(0, Tangents());
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        mesh.tangents = NotFiniteTangents();
        Refused<DisunityException>(buffer, "an edit that makes the tangents not finite");
        Refused<DisunityException>(buffer, "the same edit executed again");
        mesh.tangents = Tangents();
        mesh.vertices = Corners(4);
        var records = Execute(buffer);
        ExpectDrawnWithValidTangents(identity, records, "the corrected mesh");
        Expect.That(StoredZ(identity) == 4, "the correction's other edit is stored with it");
        var generation = Generation(identity);
        Execute(buffer);
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == generation, "once stored, neither another execution nor the flush stores it again");
        Console.WriteLine("PASS: an edit the host refuses is refused at every execution until another edit corrects it");
    }

    private static void ADestroyedMeshOrMaterialIsRefusedWhenTheBufferRuns()
    {
        var next = DisunityHost.AllocatePlayerMeshIdentity() + 1;
        var freshMesh = Triangle(0);
        var freshMeshBuffer = Drawing(freshMesh, NewMaterial());
        UnityEngine.Object.Destroy(freshMesh);
        Refused<ArgumentException>(freshMeshBuffer, "a mesh destroyed after recording, never stored");
        var goneMaterial = NewMaterial();
        var freshBuffer = Drawing(Triangle(0), goneMaterial);
        UnityEngine.Object.DestroyImmediate(goneMaterial);
        Refused<ArgumentException>(freshBuffer, "a material destroyed after recording, with a mesh never stored");
        Expect.That(DisunityHost.AllocatePlayerMeshIdentity() == next, "a refused draw gives the mesh no identity and stores nothing");
        ADestroyedMeshWithPendingEdits();
        ADestroyedMaterialWithAMeshWithPendingEdits();
        Console.WriteLine("PASS: a mesh or material destroyed after DrawMesh was recorded is refused when the buffer runs, and nothing is stored for the draw");
    }

    private static void ADestroyedMeshWithPendingEdits()
    {
        var mesh = Triangle(0);
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(5);
        UnityEngine.Object.Destroy(mesh);
        Refused<ArgumentException>(buffer, "a mesh destroyed after recording, with edits the host has not stored");
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "the refused draw did not store the destroyed mesh's edits");
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "the frame flush does not store a destroyed mesh either");
    }

    private static void ADestroyedMaterialWithAMeshWithPendingEdits()
    {
        var mesh = Triangle(0);
        var material = NewMaterial();
        var buffer = Drawing(mesh, material);
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(5);
        UnityEngine.Object.DestroyImmediate(material);
        Refused<ArgumentException>(buffer, "a material destroyed after recording, with a mesh whose edits the host has not stored");
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "the refused draw did not store the mesh's edits");
        DisunityHost.StoreChangedMeshes();
        Expect.That(StoredZ(identity) == 5 && Generation(identity) == 2, "the frame flush still stores them");
    }

    private static void ADestroyedMeshIsRefusedWhenAContextSubmitsTheBuffer()
    {
        var mesh = Triangle(0);
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(5);
        CommandWireReader.Take();
        new ScriptableRenderContext().ExecuteCommandBuffer(buffer);
        UnityEngine.Object.Destroy(mesh);
        Expect.Throws<ArgumentException>(() => new ScriptableRenderContext().Submit(), "a mesh destroyed after its buffer was queued on a context");
        Expect.That(CommandWireReader.Take().Length == 0, "the refused draw records nothing");
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "the refused draw did not store the destroyed mesh's edits");
        DisunityHost.StoreChangedMeshes();
        Console.WriteLine("PASS: a mesh destroyed after its buffer was queued on a context is refused at Submit, and nothing is stored");
    }

    private static void AMeshFirstExecutedWithNoWorldIsStoredOnceOneIsLoaded()
    {
        var mesh = TriangleWithChannels(0, Tangents());
        var buffer = Drawing(mesh, NewMaterial());
        DisunityHost.UnloadWorld();
        var refused = Expect.Catch<DisunityException>(() => Graphics.ExecuteCommandBuffer(buffer), "executing with no world loaded");
        DisunityHost.LoadWorld();
        Expect.That(refused is { Code: 3, Reason: DisunityErrorReason.InvalidArgument }, $"the host refuses the store as an invalid argument, got {refused.Code}/{refused.Reason}");
        Expect.That(refused.Message == "[3/InvalidArgument] native operation failed", $"no world holds a diagnostic to fetch, got '{refused.Message}'");
        var records = Execute(buffer);
        var identity = mesh.RendererIdentity;
        ExpectDrawnWithValidTangents(identity, records, "the mesh executed once a world is loaded");
        Expect.That(Generation(identity) == 1, "the new world stores the mesh once");
        Execute(buffer);
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == 1, "and does not store it again");
        Console.WriteLine("PASS: a mesh first executed with no world loaded is refused, then stored once when a world is loaded");
    }

    private static void AMeshKeptAcrossWorldsIsStoredInTheNewWorld()
    {
        var mesh = TriangleWithChannels(0, Tangents());
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        ReloadWorld();
        Expect.That(PlayerMeshStoreTests.Read(identity).Info.present == 0, "the new world holds nothing of the mesh yet");
        var records = Execute(buffer);
        ExpectDrawnWithValidTangents(identity, records, "the mesh executed in the new world");
        Expect.That(Generation(identity) == 1 && StoredZ(identity) == 0, "the new world holds the mesh at its first generation");
        Execute(buffer);
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == 1, "a repeat execution and the flush store nothing more");
        Console.WriteLine("PASS: a mesh kept across an unloaded world is stored, fixed data and channels, in the world loaded next, once");
    }

    private static void AnEditPendingWhenTheWorldIsUnloadedIsStoredInTheNewWorld()
    {
        var mesh = TriangleWithChannels(0, Tangents());
        var buffer = Drawing(mesh, NewMaterial());
        Execute(buffer);
        var identity = mesh.RendererIdentity;
        mesh.vertices = Corners(5);
        ReloadWorld();
        var records = Execute(buffer);
        ExpectDrawnWithValidTangents(identity, records, "the edited mesh executed in the new world");
        Expect.That(StoredZ(identity) == 5 && Generation(identity) == 1, "the new world holds the edit, stored once");
        DisunityHost.StoreChangedMeshes();
        Expect.That(Generation(identity) == 1, "the flush stores nothing more");
        Console.WriteLine("PASS: an edit that was waiting when its world was unloaded is in the world loaded next");
    }

    internal static void ReloadWorld()
    {
        var before = DisunityHost.WorldHandle;
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        Expect.That(DisunityHost.WorldHandle != before, "the world loaded next is a different world");
    }
}
