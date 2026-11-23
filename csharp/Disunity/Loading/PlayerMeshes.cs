using System.Buffers.Binary;
using System.Text.Json;
using UnityEngine;
using UnityEngine.Rendering;

namespace Disunity.Hosting;

/// <summary>Building a mesh of a resource plan: <c>{"type": "Mesh", "name", "vertex_count", "channels" [{"name", "dimension"}],
/// "sub_meshes" [{"first_index", "index_count", "topology", "base_vertex", ...}], "index_count", "index_format" (1 for 32-bit),
/// "bounds" {"center", "extent"}, "resource", "bind_poses" (optional, one array of 16 row-major floats per bone)}</c>. The
/// resource file holds, in channel order, <c>vertex_count</c> vertices of <c>dimension</c> little-endian floats per channel
/// (<c>position</c>, <c>normal</c>, <c>tangent</c>, <c>color</c>, <c>uv0</c> to <c>uv7</c>, <c>blend weights</c> and
/// <c>blend indices</c>, which make the bone weights together and are four values per vertex; a mesh whose skin channels are not keeps its vertices without a skin), then <c>index_count</c> 32-bit indices. The mesh stays readable (the engine's choice: the player keeps every
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
        var skin = new Dictionary<string, (float[] Values, int Dimension)>();
        foreach (var channel in plan.GetProperty("channels").EnumerateArray())
        {
            var dimension = channel.Int("dimension");
            var values = Read(data, ref offset, (long)vertexCount * dimension, name);
            if (channel.Str("name") is "blend weights" or "blend indices") skin[channel.Str("name")] = (values, dimension);
            else SetChannel(mesh, channel.Str("name"), values, dimension, name);
        }
        SetSkin(mesh, plan, skin);
        var indices = ReadIndices(data, ref offset, indexCount, name);
        if (offset != data.Length) throw new InvalidDataException($"mesh {name}: {data.Length - offset} bytes follow its data");
        SetSubMeshes(mesh, plan, indices);
        mesh.bounds = ReadBounds(plan.GetProperty("bounds"));
        return mesh;
    }

    /// <summary>Sets the bone weights from the blend weight and blend index channels, which come together, and the bind poses. A
    /// mesh whose skin channels cannot make the bone weights keeps its vertices and gets no bone weights and no bind poses: a
    /// skinned renderer of it has no placement and is left out of frames, and the log names the mesh and why.</summary>
    private static void SetSkin(Mesh mesh, JsonElement plan, Dictionary<string, (float[] Values, int Dimension)> channels)
    {
        if (SkinFlaw(mesh, channels) is { } flaw)
        {
            Debug.LogWarning($"The skin of mesh {mesh.name} is not built and the skinned renderers of it are left out: {flaw}");
            return;
        }
        if (channels.Count == 2)
        {
            var (weights, joints) = (channels["blend weights"].Values, channels["blend indices"].Values);
            mesh.boneWeights = Enumerable.Range(0, weights.Length / 4).Select(i => new BoneWeight
            {
                weight0 = weights[i * 4], weight1 = weights[i * 4 + 1], weight2 = weights[i * 4 + 2], weight3 = weights[i * 4 + 3],
                boneIndex0 = (int)joints[i * 4], boneIndex1 = (int)joints[i * 4 + 1], boneIndex2 = (int)joints[i * 4 + 2], boneIndex3 = (int)joints[i * 4 + 3],
            }).ToArray();
        }
        if (plan.TryGetProperty("bind_poses", out var poses)) mesh.bindposes = poses.EnumerateArray().Select(BindPose).ToArray();
    }

    /// <summary>Why the skin channels cannot make bone weights, or null when there are none or both are four values per vertex.</summary>
    private static string? SkinFlaw(Mesh mesh, Dictionary<string, (float[] Values, int Dimension)> channels)
    {
        if (channels.Count == 1) return "blend weights and blend indices come together";
        foreach (var (name, (values, dimension)) in channels)
        {
            if (dimension != 4) return $"{name} has {dimension} components, not 4";
            if (values.Length != mesh.vertexCount * 4) return $"{name} does not match the {mesh.vertexCount} vertices";
        }
        return null;
    }

    /// <summary>A bind pose from its 16 row-major floats.</summary>
    private static Matrix4x4 BindPose(JsonElement values)
    {
        var pose = new Matrix4x4();
        for (var row = 0; row < 4; row++)
            for (var column = 0; column < 4; column++) pose[row, column] = values[row * 4 + column].GetSingle();
        return pose;
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
