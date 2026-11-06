using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>Every vertex channel and sub-mesh of a mesh, which the host reads beside the fixed vertices.</summary>
public partial class Mesh
{
    private const uint AttributePosition = 0, AttributeNormal = 1, AttributeTangent = 2, AttributeColor = 3, AttributeUv0 = 4;

    /// <summary>Sends every channel the mesh holds and every sub-mesh, empty ones included so range i is sub-mesh i, with
    /// all indices and their base vertex applied. Called right after the fixed store of the same data, which it pairs with.</summary>
    private unsafe void StoreChannelsForRenderer(MeshData data)
    {
        var channels = new List<DisunityMeshChannel>();
        var floats = new List<float>();
        AddChannels(data, channels, floats);
        var indices = new List<uint>();
        var ranges = new List<DisunityMeshRange>();
        foreach (var (subIndices, topology, baseVertex) in data.SubMeshes)
        {
            ranges.Add(new DisunityMeshRange { start = (uint)indices.Count, count = (uint)subIndices.Length, base_vertex = (uint)Math.Max(baseVertex, 0), topology = TopologyCode(topology) });
            foreach (var index in subIndices) indices.Add(unchecked((uint)(index + baseVertex)));
        }
        var info = new DisunityMeshChannelsInfo
        {
            identity = _identity, vertex_count = (uint)data.Positions.Length, channel_count = (uint)channels.Count,
            float_count = (uint)floats.Count, index_count = (uint)indices.Count, range_count = (uint)ranges.Count,
        };
        var channelArray = channels.ToArray();
        var floatArray = floats.ToArray();
        var indexArray = indices.ToArray();
        var rangeArray = ranges.ToArray();
        fixed (DisunityMeshChannel* channelPointer = channelArray)
        fixed (float* floatPointer = floatArray)
        fixed (uint* indexPointer = indexArray)
        fixed (DisunityMeshRange* rangePointer = rangeArray)
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_channels_store(DisunityHost.WorldHandle, &info, channelPointer, floatPointer, indexPointer, rangePointer));
    }

    private static void AddChannels(MeshData data, List<DisunityMeshChannel> channels, List<float> floats)
    {
        var count = data.Positions.Length;
        if (count == 0) return;
        Add(channels, floats, AttributePosition, 3, data.Positions, v => [v.x, v.y, v.z]);
        if (data.Normals.Length == count) Add(channels, floats, AttributeNormal, 3, data.Normals, v => [v.x, v.y, v.z]);
        if (data.Tangents.Length == count) Add(channels, floats, AttributeTangent, 4, data.Tangents, v => [v.x, v.y, v.z, v.w]);
        if (data.Colors.Length == count) Add(channels, floats, AttributeColor, 4, data.Colors, v => [v.r, v.g, v.b, v.a]);
        for (var uv = 0; uv < data.Uvs.Length; uv++)
        {
            var dimension = data.UvDimensions[uv];
            if (dimension == 0 || data.Uvs[uv].Length != count) continue;
            Add(channels, floats, AttributeUv0 + (uint)uv, (uint)dimension, data.Uvs[uv], v => Leading(v, dimension));
        }
    }

    private static float[] Leading(Vector4 value, int dimension) => new[] { value.x, value.y, value.z, value.w }[..dimension];

    private static void Add<T>(List<DisunityMeshChannel> channels, List<float> floats, uint number, uint dimension, T[] values, Func<T, float[]> parts)
    {
        channels.Add(new DisunityMeshChannel { number = number, dimension = dimension, offset = (uint)floats.Count });
        foreach (var value in values) floats.AddRange(parts(value));
    }

    /// <summary>The host's numbering of sub-mesh topologies: 0 triangles, 1 quads, 2 lines, 3 line strip, 4 points.</summary>
    private static uint TopologyCode(MeshTopology topology) => topology switch
    {
        MeshTopology.Triangles => 0,
        MeshTopology.Quads => 1,
        MeshTopology.Lines => 2,
        MeshTopology.LineStrip => 3,
        MeshTopology.Points => 4,
        _ => throw new NotSupportedException($"mesh topology {topology} is not one the host knows"),
    };
}
