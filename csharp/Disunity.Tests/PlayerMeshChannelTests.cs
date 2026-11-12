using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>A stored mesh also reaches the host with every vertex channel it holds and every sub-mesh, whatever its topology.</summary>
internal static class PlayerMeshChannelTests
{
    internal const int Position = 0, Normal = 1, Tangent = 2, Colour = 3, Uv0 = 4, Uv1 = 5, Uv2 = 6;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            StoresTangentsSecondUvsAndColoursWithTheirDimensions();
            StoresOnlyTheChannelsTheMeshHolds();
            StoresEverySubMeshWithItsTopology();
            StoresChannelsAgainWhenTheMeshChanges();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    internal sealed record Stored(DisunityMeshChannelsInfo Info, DisunityMeshChannel[] Channels, float[] Data, uint[] Indices, DisunityMeshRange[] Ranges)
    {
        public float[] Values(int number)
        {
            var channel = Channels.Single(c => c.number == number);
            return Data.Skip((int)channel.offset).Take((int)(channel.dimension * Info.vertex_count)).ToArray();
        }

        public uint Dimension(int number) => Channels.Single(c => c.number == number).dimension;

        public bool Has(int number) => Channels.Any(c => c.number == number);
    }

    internal static unsafe Stored Read(ulong identity)
    {
        var info = new DisunityMeshChannelsInfo { identity = identity };
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_channels_read(DisunityHost.WorldHandle, &info, null, null, null, null));
        var channels = new DisunityMeshChannel[info.channel_count];
        var data = new float[info.float_count];
        var indices = new uint[info.index_count];
        var ranges = new DisunityMeshRange[info.range_count];
        var request = info;
        request.generation = 0;
        fixed (DisunityMeshChannel* channelPointer = channels)
        fixed (float* dataPointer = data)
        fixed (uint* indexPointer = indices)
        fixed (DisunityMeshRange* rangePointer = ranges)
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_channels_read(DisunityHost.WorldHandle, &request, channelPointer, dataPointer, indexPointer, rangePointer));
        return new Stored(info, channels, data, indices, ranges);
    }

    private static unsafe ulong FixedGeneration(ulong identity)
    {
        var info = new DisunityMeshInfo();
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_read(DisunityHost.WorldHandle, identity, &info, null, null, null));
        return info.generation;
    }

    private static Mesh Triangle() => new()
    {
        vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0)],
        triangles = [0, 1, 2],
    };

    private static void StoresTangentsSecondUvsAndColoursWithTheirDimensions()
    {
        var mesh = Triangle();
        mesh.normals = [Vector3.back, Vector3.back, Vector3.back];
        mesh.tangents = [new Vector4(1, 0, 0, 1), new Vector4(0, 1, 0, -1), new Vector4(0, 0, 1, 1)];
        mesh.colors = [new Color(1, 0, 0, 0.25f), new Color(0, 1, 0, 0.5f), new Color(0, 0, 1, 0.75f)];
        mesh.uv = [new Vector2(0, 0), new Vector2(1, 0), new Vector2(0, 1)];
        mesh.uv2 = [new Vector2(0.25f, 0.5f), new Vector2(0.75f, 0.125f), new Vector2(1, 1)];
        mesh.SetUVs(2, new[] { new Vector3(1, 2, 3), new Vector3(4, 5, 6), new Vector3(7, 8, 9) });
        var stored = Read(mesh.RendererIdentity);
        Expect.That(stored.Info.present == 1 && stored.Info.vertex_count == 3, "the channels are stored with the mesh's vertex count");
        Expect.Sequence(new[] { Position, Normal, Tangent, Colour, Uv0, Uv1, Uv2 }, stored.Channels.Select(c => (int)c.number).OrderBy(n => n), "every channel the mesh holds is stored");
        Expect.That(stored.Dimension(Position) == 3 && stored.Dimension(Normal) == 3 && stored.Dimension(Tangent) == 4 && stored.Dimension(Colour) == 4,
            "positions and normals have three floats, tangents and colours four");
        Expect.That(stored.Dimension(Uv0) == 2 && stored.Dimension(Uv1) == 2 && stored.Dimension(Uv2) == 3, "a texture coordinate keeps the dimension it was set with");
        Expect.Sequence(new float[] { 1, 0, 0, 1, 0, 1, 0, -1, 0, 0, 1, 1 }, stored.Values(Tangent), "tangents keep their w");
        Expect.Sequence(new[] { 1f, 0, 0, 0.25f, 0, 1, 0, 0.5f, 0, 0, 1, 0.75f }, stored.Values(Colour), "colours are stored as RGBA floats");
        Expect.Sequence(new[] { 0.25f, 0.5f, 0.75f, 0.125f, 1f, 1f }, stored.Values(Uv1), "the second texture coordinates are stored");
        Expect.Sequence(new float[] { 1, 2, 3, 4, 5, 6, 7, 8, 9 }, stored.Values(Uv2), "a three-dimensional texture coordinate keeps all three floats");
        Expect.Sequence(new float[] { 0, 0, 0, 1, 0, 0, 0, 1, 0 }, stored.Values(Position), "positions are stored in order");
        Console.WriteLine("PASS: a stored mesh sends its tangents, second texture coordinates and colours as channels 2, 3 and 5 with their dimensions");
    }

    private static void StoresOnlyTheChannelsTheMeshHolds()
    {
        var stored = Read(Triangle().RendererIdentity);
        Expect.Sequence(new[] { Position }, stored.Channels.Select(c => (int)c.number), "a mesh with only positions sends only channel 0");
        var empty = Read(new Mesh().RendererIdentity);
        Expect.That(empty.Info.present == 1 && empty.Channels.Length == 0 && empty.Ranges.Length == 1 && empty.Ranges[0].count == 0,
            "an empty mesh sends no channels and its empty sub-mesh");
        Console.WriteLine("PASS: a stored mesh sends only the channels it holds");
    }

    private static void StoresEverySubMeshWithItsTopology()
    {
        var mesh = new Mesh { vertices = [Vector3.zero, Vector3.right, Vector3.up, Vector3.one, Vector3.forward, Vector3.back] };
        mesh.subMeshCount = 6;
        mesh.SetTriangles(new[] { 0, 1, 2 }, 0);
        mesh.SetIndices(new[] { 0, 1, 2, 3 }, MeshTopology.Quads, 1);
        mesh.SetIndices(new[] { 0, 1, 1, 2 }, MeshTopology.Lines, 2);
        mesh.SetIndices(new[] { 0, 1, 2 }, MeshTopology.LineStrip, 3);
        mesh.SetIndices(new[] { 5, 4 }, MeshTopology.Points, 4);
        mesh.SetTriangles(new[] { 0, 1, 2 }, 5, true, 3);
        var stored = Read(mesh.RendererIdentity);
        Expect.That(stored.Ranges.Length == 6, "every sub-mesh has a range, so range i is sub-mesh i");
        Expect.Sequence(new uint[] { 0, 1, 2, 3, 4, 0 }, stored.Ranges.Select(r => r.topology), "triangles, quads, lines, line strip and points are 0 to 4, in the engine's order");
        Expect.Sequence(new uint[] { 3, 4, 4, 3, 2, 3 }, stored.Ranges.Select(r => r.count), "each range counts its indices");
        Expect.Sequence(new uint[] { 0, 3, 7, 11, 14, 16 }, stored.Ranges.Select(r => r.start), "ranges follow each other in the index buffer");
        Expect.That(stored.Ranges[5].base_vertex == 3, "a range reports its base vertex");
        Expect.Sequence(new uint[] { 0, 1, 2, 0, 1, 2, 3, 0, 1, 1, 2, 0, 1, 2, 5, 4, 3, 4, 5 }, stored.Indices, "all indices are stored, with the base vertex applied");
        Console.WriteLine("PASS: a stored mesh sends sub-meshes of every topology, a line sub-mesh as topology 2");
    }

    private static void StoresChannelsAgainWhenTheMeshChanges()
    {
        var mesh = Triangle();
        var identity = mesh.RendererIdentity;
        mesh.uv2 = [Vector2.zero, Vector2.one, Vector2.up];
        Expect.That(!Read(identity).Has(Uv1), "a change is not stored until the frame is about to be drawn");
        DisunityHost.StoreChangedMeshes();
        var stored = Read(identity);
        Expect.That(stored.Has(Uv1), "the changed mesh sends its new channel");
        Expect.That(stored.Info.generation == FixedGeneration(identity) && stored.Info.generation == 2, "the channels carry the generation of the fixed mesh they were stored with");
        mesh.tangents = [new Vector4(0, 0, 1, 1), new Vector4(0, 0, 1, 1), new Vector4(0, 0, 1, -1)];
        DisunityHost.StoreChangedMeshes();
        var tangents = Read(identity);
        Expect.That(tangents.Has(Tangent) && tangents.Info.generation == 3, "a tangent change alone stores the mesh again");
        Expect.Sequence(new float[] { 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, -1 }, tangents.Values(Tangent), "the new tangents are stored");
        Console.WriteLine("PASS: the channels are stored again with the fixed mesh and agree with its generation");
    }
}
