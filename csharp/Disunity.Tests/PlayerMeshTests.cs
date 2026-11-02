using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>A mesh a filter holds reaches the host with its data and again, once per frame, after it changes; a
/// renderer's materials give the host a colour and texture identity per slot.</summary>
internal static class PlayerMeshTests
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
            SendsColourAndTexturePerSlot();
            DropsTheSlotsTheMaterialsNoLongerHold();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static GameObject NewObject() => new("mesh object");

    private static unsafe (DisunityMeshInfo Info, DisunityMeshVertex[] Vertices, uint[] Indices, DisunityMeshSubMesh[] Runs) Read(ulong identity)
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

    private static unsafe ulong ReadDrawn(MeshFilter filter)
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
        mesh.subMeshCount = 3;
        mesh.SetTriangles(new[] { 0, 1, 2 }, 0);
        mesh.SetIndices(new[] { 0, 1, 2, 3 }, MeshTopology.Quads, 1);
        mesh.SetTriangles(new[] { 0, 1, 2 }, 2, true, 3);
        var (info, _, indices, runs) = Read(mesh.RendererIdentity);
        Expect.That(info.left_out == 1, "a quad sub-mesh is counted as left out");
        Expect.That(info.submesh_count == 2 && runs[0].slot == 0 && runs[1].slot == 2, "the runs keep their sub-mesh numbers, with a gap for the left-out one");
        Expect.Sequence(new uint[] { 0, 1, 2, 3, 4, 5 }, indices, "the base vertex is applied to the indices");
        Expect.That(runs[1].first_index == 3 && runs[1].index_count == 3, "a later run starts after the earlier ones");
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

    private static unsafe DisunityDrawnMaterial[] ReadSlots(Renderer renderer)
    {
        uint count;
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_slots_read(DisunityHost.WorldHandle, renderer.Handle, null, 0, &count));
        var slots = new DisunityDrawnMaterial[count];
        fixed (DisunityDrawnMaterial* pointer = slots)
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_slots_read(DisunityHost.WorldHandle, renderer.Handle, pointer, count, &count));
        return slots;
    }

    /// <summary>A material on a shader that declares no properties, holding only <paramref name="property"/>.</summary>
    private static Material Coloured(string property, Color color)
    {
        var material = new Material(DisunityHost.RegisterShader("Test/PlayerMesh/Plain"));
        material.SetColor(property, color);
        return material;
    }

    private static void SendsColourAndTexturePerSlot()
    {
        var renderer = NewObject().AddComponent<MeshRenderer>();
        var texture = new Texture2D(2, 2);
        var textured = Coloured("_Color", new Color(0.1f, 0.2f, 0.3f, 0.4f));
        textured.mainTexture = texture;
        var based = Coloured("_BaseColor", new Color(0.5f, 0.6f, 0.7f, 1f));
        var plain = new Material(DisunityHost.RegisterShader("Test/PlayerMesh/Plain"));
        var undrawn = new Material(DisunityHost.RegisterShader("Test/PlayerMesh/Plain")) { mainTexture = new Texture2D(0, 0) };
        renderer.sharedMaterials = [textured, based, plain, null, undrawn];
        var slots = ReadSlots(renderer);
        Expect.That(slots.Length == 5, "one slot per material");
        Expect.That(Math.Abs(slots[0].r - 0.1f) < 1e-6f && Math.Abs(slots[0].a - 0.4f) < 1e-6f && slots[0].texture == texture.RendererIdentity,
            "a material sends its _Color and the identity of its Texture2D");
        Expect.That(Math.Abs(slots[1].g - 0.6f) < 1e-6f && slots[1].texture == 0, "_BaseColor is used when there is no _Color");
        Expect.That(slots[2].r == 1 && slots[2].g == 1 && slots[2].b == 1 && slots[2].a == 1 && slots[2].texture == 0, "a material with neither colour is white");
        Expect.That(slots[3].r == 1 && slots[3].texture == 0, "an empty slot is white and untextured");
        Expect.That(slots[4].texture == 0, "a texture of no size sends no texture");
        Expect.That(texture.RendererIdentity >= DisunityHost.FirstPlayerTextureIdentity, "the texture identity is a player texture's");
        renderer.sharedMaterial = Coloured("_Color", new Color(-1f, float.NaN, 2f, 1f));
        var clamped = ReadSlots(renderer);
        Expect.That(clamped.Length == 5 && clamped[0].r == 0 && clamped[0].g == 1 && clamped[0].b == 2, "negative channels clamp to 0 and a value that is not a number is 1");
        Console.WriteLine("PASS: a renderer sends each slot's colour and player texture identity when its materials are set");
    }

    private static void DropsTheSlotsTheMaterialsNoLongerHold()
    {
        var renderer = NewObject().AddComponent<MeshRenderer>();
        renderer.sharedMaterials = [Coloured("_Color", Color.red), Coloured("_Color", Color.blue)];
        Expect.That(ReadSlots(renderer).Length == 2, "two materials, two slots");
        renderer.sharedMaterials = [Coloured("_Color", Color.green)];
        var slots = ReadSlots(renderer);
        Expect.That(slots.Length == 1 && slots[0].g == 1 && slots[0].r == 0, "setting fewer materials replaces the slots");
        renderer.sharedMaterials = [];
        Expect.That(ReadSlots(renderer).Length == 0, "no materials, no slots");
        Console.WriteLine("PASS: setting the materials replaces the slots");
    }
}
