using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>A mesh a filter holds reaches the host with its data and again, once per frame, after it changes; a
/// renderer's materials reach the host as world materials, which draw the slots (see PlayerMeshLiveMaterialTests).</summary>
internal static class PlayerMeshStoreTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            StoresTheMeshWhenAFilterTakesIt();
            StoresChangesOncePerFlush();
            LeavesOutOtherTopologiesAndAppliesBaseVertex();
            GivesEachMeshOneIdentityAboveTheTextures();
            ClearsTheDrawnMeshWithTheFilter();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static GameObject NewObject() => new("mesh object");

    internal static unsafe (DisunityMeshInfo Info, DisunityMeshVertex[] Vertices, uint[] Indices, DisunityMeshSubMesh[] Runs) Read(ulong identity)
    {
        var info = new DisunityMeshInfo();
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_read(DisunityHost.WorldHandle, identity, &info, null, null, null));
        var vertices = new DisunityMeshVertex[info.vertex_count];
        var indices = new uint[info.index_count];
        var runs = new DisunityMeshSubMesh[info.submesh_count];
        var request = info;
        request.generation = 0;
        fixed (DisunityMeshVertex* vertexPointer = vertices)
        fixed (uint* indexPointer = indices)
        fixed (DisunityMeshSubMesh* runPointer = runs)
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_read(DisunityHost.WorldHandle, identity, &request, vertexPointer, indexPointer, runPointer));
        return (info, vertices, indices, runs);
    }

    private static Mesh Quad() => new()
    {
        vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(1, 1, 0), new Vector3(0, 1, 0)],
        normals = [Vector3.back, Vector3.back, Vector3.back, Vector3.back],
        uv = [new Vector2(0, 0), new Vector2(1, 0), new Vector2(1, 1), new Vector2(0, 1)],
        colors = [Color.red, Color.green, Color.blue, Color.white],
        triangles = [0, 2, 1, 0, 3, 2],
    };

    internal static unsafe ulong ReadDrawn(MeshFilter filter)
    {
        ulong identity;
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_filter_drawn_read(DisunityHost.WorldHandle, filter.Handle, &identity));
        return identity;
    }

    private static void StoresTheMeshWhenAFilterTakesIt()
    {
        var standalone = Quad();
        Expect.That(Read(standalone.RendererIdentity).Info.present == 1, "asking for the identity stores the mesh");
        var mesh = Quad();
        var filter = NewObject().AddComponent<MeshFilter>();
        filter.sharedMesh = mesh;
        var identity = mesh.RendererIdentity;
        var (info, vertices, indices, runs) = Read(identity);
        Expect.That(info.present == 1 && info.generation == 1, "assigning a mesh to a filter stores it once");
        Expect.That(info.vertex_count == 4 && info.index_count == 6 && info.submesh_count == 1 && info.channels == 7 && info.left_out == 0,
            $"the description counts vertices, indices and sub-meshes and names all three channels, got {info.vertex_count}/{info.index_count}/{info.submesh_count}/{info.channels}");
        Expect.That(vertices[2].x == 1 && vertices[2].y == 1 && vertices[2].z == 0, "positions are stored in order");
        Expect.That(vertices[1].nz == -1 && vertices[1].u == 1 && vertices[1].v == 0, "normals and the first uv are stored");
        Expect.That(vertices[1].r == 0 && vertices[1].g == 1 && vertices[1].b == 0 && vertices[1].a == 1, "colours are stored");
        Expect.Sequence(new uint[] { 0, 2, 1, 0, 3, 2 }, indices, "triangle indices are stored");
        Expect.That(runs.Length == 1 && runs[0].slot == 0 && runs[0].first_index == 0 && runs[0].index_count == 6, "the sub-mesh is a run of its indices");
        Expect.That(ReadDrawn(filter) == identity, "the filter records the identity of its mesh");
        var bare = new Mesh { vertices = [Vector3.zero, Vector3.right, Vector3.up], triangles = [0, 1, 2] };
        Expect.That(Read(bare.RendererIdentity).Info.channels == 0, "a mesh without normals, uvs or colours stores none of them");
        Console.WriteLine("PASS: a mesh a filter holds is stored with its positions, normals, uvs, colours, indices and sub-mesh runs");
    }

    private static void StoresChangesOncePerFlush()
    {
        var mesh = Quad();
        var identity = mesh.RendererIdentity;
        mesh.vertices = [new Vector3(0, 0, 5), new Vector3(1, 0, 5), new Vector3(1, 1, 5), new Vector3(0, 1, 5)];
        mesh.uv = [Vector2.zero, Vector2.zero, Vector2.zero, Vector2.zero];
        mesh.SetTriangles(new[] { 0, 1, 2 }, 0);
        Expect.That(Read(identity).Info.generation == 1, "changes are not stored until the frame is about to be drawn");
        DisunityHost.StoreChangedMeshes();
        var (info, vertices, indices, _) = Read(identity);
        Expect.That(info.generation == 2, "several edits store the mesh once");
        Expect.That(vertices[0].z == 5 && indices.Length == 3 && vertices[1].u == 0, "the stored data is the edited data");
        DisunityHost.StoreChangedMeshes();
        Expect.That(Read(identity).Info.generation == 2, "a mesh that did not change is not stored again");
        mesh.RecalculateNormals();
        mesh.subMeshCount = 2;
        DisunityHost.StoreChangedMeshes();
        Expect.That(Read(identity).Info.generation == 3, "recalculating normals and adding a sub-mesh store it");
        mesh.UploadMeshData(false);
        DisunityHost.StoreChangedMeshes();
        Expect.That(Read(identity).Info.generation == 4, "UploadMeshData stores the mesh again");
        mesh.Clear();
        DisunityHost.StoreChangedMeshes();
        var cleared = Read(identity).Info;
        Expect.That(cleared.vertex_count == 0 && cleared.generation == 5, "Clear stores the empty mesh");
        var before = DisunityHost.AllocatePlayerMeshIdentity();
        var untouched = Quad();
        untouched.vertices = [Vector3.zero, Vector3.one, Vector3.up, Vector3.right];
        DisunityHost.StoreChangedMeshes();
        Expect.That(DisunityHost.AllocatePlayerMeshIdentity() == before + 1, "editing a mesh without an identity gives it none");
        Console.WriteLine("PASS: edits to a stored mesh are stored once before the next frame, and only for meshes with an identity");
    }

    private static void LeavesOutOtherTopologiesAndAppliesBaseVertex()
    {
        var mesh = new Mesh { vertices = [Vector3.zero, Vector3.right, Vector3.up, Vector3.one, Vector3.forward, Vector3.back] };
        mesh.subMeshCount = 4;
        mesh.SetTriangles(new[] { 0, 1, 2 }, 0);
        mesh.SetIndices(new[] { 0, 1, 2, 3 }, MeshTopology.Quads, 1);
        mesh.SetTriangles(new[] { 0, 1, 2 }, 2, true, 3);
        var (info, _, indices, runs) = Read(mesh.RendererIdentity);
        Expect.That(info.left_out == 1, "a quad sub-mesh is counted as left out");
        Expect.That(info.submesh_count == 2 && runs[0].slot == 0 && runs[1].slot == 2, "the runs keep their sub-mesh numbers, with a gap for the left-out one");
        Expect.Sequence(new uint[] { 0, 1, 2, 3, 4, 5 }, indices, "the base vertex is applied to the indices");
        Expect.That(runs[1].first_index == 3 && runs[1].index_count == 3, "a later run starts after the earlier ones");
        Expect.That(info.index_count == 6, "the empty sub-mesh after them makes no run");
        Console.WriteLine("PASS: sub-meshes of other topologies are left out and counted, and base vertices are applied");
    }

    private static void GivesEachMeshOneIdentityAboveTheTextures()
    {
        var first = Quad();
        var second = Quad();
        Expect.That(first.RendererIdentity != second.RendererIdentity, "meshes get different identities");
        Expect.That(first.RendererIdentity == first.RendererIdentity, "a mesh keeps its identity");
        Expect.That(first.RendererIdentity >= DisunityHost.FirstPlayerMeshIdentity && DisunityHost.FirstPlayerMeshIdentity > DisunityHost.FirstPlayerTextureIdentity,
            "mesh identities start above the texture identities");
        Console.WriteLine("PASS: each mesh has one identity, from a range above the textures'");
    }

    private static void ClearsTheDrawnMeshWithTheFilter()
    {
        var filter = NewObject().AddComponent<MeshFilter>();
        var mesh = Quad();
        filter.sharedMesh = mesh;
        Expect.That(ReadDrawn(filter) == mesh.RendererIdentity, "the filter names the mesh's identity");
        var other = Quad();
        filter.sharedMesh = other;
        Expect.That(ReadDrawn(filter) == other.RendererIdentity, "naming another mesh changes it");
        filter.sharedMesh = null;
        Expect.That(ReadDrawn(filter) == 0, "no mesh draws nothing");
        Console.WriteLine("PASS: a filter's drawn mesh follows its shared mesh");
    }
}
