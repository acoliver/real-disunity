using System.Buffers.Binary;
using System.Text.Json;
using UnityEngine;
using UnityEngine.Rendering;

namespace Disunity.Hosting;

/// <summary>Building a mesh of a resource plan: <c>{"type": "Mesh", "name", "vertex_count", "channels" [{"name", "dimension"}],
/// "sub_meshes" [{"first_index", "index_count", "topology", "base_vertex", ...}], "index_count", "index_format" (1 for 32-bit),
/// "bounds" {"center", "extent"}, "resource"}</c>. The resource file holds, in channel order, <c>vertex_count</c> vertices of
/// <c>dimension</c> little-endian floats per channel (<c>position</c>, <c>normal</c>, <c>tangent</c>, <c>color</c>, <c>uv0</c> to
/// <c>uv7</c>), then <c>index_count</c> 32-bit indices. The mesh stays readable (the engine's choice: the player keeps every
/// mesh readable), and every filter that names it shares the one object.</summary>
internal static class PlayerMeshes
{
    private const int UvChannelCount = 8;

    /// <summary>Builds the mesh whose data file lies under <paramref name="directory"/>.</summary>
    public static Mesh Build(JsonElement plan, string directory)
    {
        var name = plan.Str("name");
        var data = File.ReadAllBytes(Path.Combine(directory, plan.Str("resource")));
        var vertexCount = plan.Int("vertex_count");
        var indexCount = plan.Int("index_count");
        var offset = 0;
        var mesh = new Mesh { name = name };
        foreach (var channel in plan.GetProperty("channels").EnumerateArray())
        {
            var dimension = channel.Int("dimension");
            var values = Read(data, ref offset, (long)vertexCount * dimension, name);
            SetChannel(mesh, channel.Str("name"), values, dimension, name);
        }
        var indices = ReadIndices(data, ref offset, indexCount, name);
        if (offset != data.Length) throw new InvalidDataException($"mesh {name}: {data.Length - offset} bytes follow its data");
        SetSubMeshes(mesh, plan, indices);
        mesh.bounds = ReadBounds(plan.GetProperty("bounds"));
        return mesh;
    }

    private static float[] Read(byte[] data, ref int offset, long count, string mesh)
    {
        if (count < 0 || offset + count * 4 > data.Length) throw new InvalidDataException($"mesh {mesh}: its data is shorter than its channels");
        var values = new float[count];
        for (var i = 0; i < values.Length; i++, offset += 4) values[i] = BinaryPrimitives.ReadSingleLittleEndian(data.AsSpan(offset, 4));
        return values;
    }

    private static int[] ReadIndices(byte[] data, ref int offset, int count, string mesh)
    {
        if (count < 0 || offset + (long)count * 4 > data.Length) throw new InvalidDataException($"mesh {mesh}: its data is shorter than its indices");
        var indices = new int[count];
        for (var i = 0; i < indices.Length; i++, offset += 4) indices[i] = BinaryPrimitives.ReadInt32LittleEndian(data.AsSpan(offset, 4));
        return indices;
    }

    private static void SetChannel(Mesh mesh, string channel, float[] values, int dimension, string name)
    {
        switch (channel)
        {
            case "position": mesh.vertices = Vectors3(values); break;
            case "normal": mesh.normals = Vectors3(values); break;
            case "tangent": mesh.tangents = Vectors4(values); break;
            case "color": mesh.colors = Colors(values); break;
            default: SetUv(mesh, channel, values, dimension, name); break;
        }
    }

    private static void SetUv(Mesh mesh, string channel, float[] values, int dimension, string name)
    {
        if (!channel.StartsWith("uv", StringComparison.Ordinal) || !int.TryParse(channel.AsSpan(2), out var index) || index >= UvChannelCount)
            throw new InvalidDataException($"mesh {name}: channel {channel} is not built");
        switch (dimension)
        {
            case 2: mesh.SetUVs(index, Vectors2(values)); break;
            case 3: mesh.SetUVs(index, Vectors3(values)); break;
            case 4: mesh.SetUVs(index, Vectors4(values)); break;
            default: throw new InvalidDataException($"mesh {name}: channel {channel} has {dimension} components");
        }
    }

    private static Vector2[] Vectors2(float[] v) => Enumerable.Range(0, v.Length / 2).Select(i => new Vector2(v[i * 2], v[i * 2 + 1])).ToArray();

    private static Vector3[] Vectors3(float[] v) => Enumerable.Range(0, v.Length / 3).Select(i => new Vector3(v[i * 3], v[i * 3 + 1], v[i * 3 + 2])).ToArray();

    private static Vector4[] Vectors4(float[] v) => Enumerable.Range(0, v.Length / 4).Select(i => new Vector4(v[i * 4], v[i * 4 + 1], v[i * 4 + 2], v[i * 4 + 3])).ToArray();

    private static Color[] Colors(float[] v) => Enumerable.Range(0, v.Length / 4).Select(i => new Color(v[i * 4], v[i * 4 + 1], v[i * 4 + 2], v[i * 4 + 3])).ToArray();

    private static void SetSubMeshes(Mesh mesh, JsonElement plan, int[] indices)
    {
        if (plan.Int("index_format") == 1) mesh.indexFormat = IndexFormat.UInt32;
        var subMeshes = plan.GetProperty("sub_meshes");
        mesh.subMeshCount = subMeshes.GetArrayLength();
        var number = 0;
        foreach (var sub in subMeshes.EnumerateArray())
        {
            var slice = indices.AsSpan(sub.Int("first_index"), sub.Int("index_count")).ToArray();
            mesh.SetIndices(slice, (MeshTopology)sub.Int("topology"), number++, false, sub.Int("base_vertex"));
        }
    }

    private static Bounds ReadBounds(JsonElement bounds)
    {
        var center = bounds.GetProperty("center");
        var extent = bounds.GetProperty("extent");
        return new Bounds(new Vector3(center[0].GetSingle(), center[1].GetSingle(), center[2].GetSingle()),
            new Vector3(extent[0].GetSingle() * 2, extent[1].GetSingle() * 2, extent[2].GetSingle() * 2));
    }
}
