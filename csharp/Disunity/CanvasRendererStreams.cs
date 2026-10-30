namespace UnityEngine;

/// <summary>Conversions between a list of <see cref="UIVertex"/> and the separate channel lists a mesh takes.
/// They are plain data and touch no renderer.</summary>
public sealed partial class CanvasRenderer
{
    /// <summary>Turns a vertex stream into channel lists and indices. Every output list is cleared first. Each
    /// input vertex becomes one entry in every list, with no sharing of equal vertices, so the indices are
    /// 0 through <c>verts.Count - 1</c>: the stream is a triangle list in the order given.</summary>
    public static void SplitUIVertexStreams(List<UIVertex> verts, List<Vector3> positions, List<Color32> colors,
        List<Vector4> uv0S, List<Vector4> uv1S, List<Vector4> uv2S, List<Vector4> uv3S, List<Vector3> normals,
        List<Vector4> tangents, List<int> indices)
    {
        foreach (var list in new System.Collections.IList?[] { verts, positions, colors, uv0S, uv1S, uv2S, uv3S, normals, tangents, indices })
            if (list is null) throw new ArgumentNullException("lists", "SplitUIVertexStreams takes every list");
        positions.Clear(); colors.Clear(); uv0S.Clear(); uv1S.Clear(); uv2S.Clear(); uv3S.Clear();
        normals.Clear(); tangents.Clear(); indices.Clear();
        for (var i = 0; i < verts.Count; i++)
        {
            var v = verts[i];
            positions.Add(v.position); colors.Add(v.color); uv0S.Add(v.uv0); uv1S.Add(v.uv1); uv2S.Add(v.uv2);
            uv3S.Add(v.uv3); normals.Add(v.normal); tangents.Add(v.tangent); indices.Add(i);
        }
    }

    /// <summary>Builds a vertex stream with one vertex per index, in index order, so shared vertices repeat.
    /// <paramref name="verts"/> is cleared first. A channel list that is null or empty gives the
    /// <see cref="UIVertex.simpleVert"/> value for that channel, and a channel that is not empty must be as long as
    /// <paramref name="positions"/>.</summary>
    public static void CreateUIVertexStream(List<UIVertex> verts, List<Vector3> positions, List<Color32> colors,
        List<Vector4> uv0S, List<Vector4> uv1S, List<Vector4> uv2S, List<Vector4> uv3S, List<Vector3> normals,
        List<Vector4> tangents, List<int> indices)
    {
        ArgumentNullException.ThrowIfNull(verts);
        ArgumentNullException.ThrowIfNull(indices);
        var all = new List<UIVertex>();
        AddUIVertexStream(all, positions, colors, uv0S, uv1S, uv2S, uv3S, normals, tangents);
        var stream = new List<UIVertex>(indices.Count);
        foreach (var index in indices)
        {
            if ((uint)index >= (uint)all.Count)
                throw new ArgumentOutOfRangeException(nameof(indices), index, $"an index must be below the vertex count, {all.Count}");
            stream.Add(all[index]);
        }
        verts.Clear();
        verts.AddRange(stream);
    }

    /// <summary>Appends one vertex per position to <paramref name="verts"/>. A channel list that is null or empty
    /// gives the <see cref="UIVertex.simpleVert"/> value for that channel, and a channel that is not empty must be
    /// as long as <paramref name="positions"/>.</summary>
    public static void AddUIVertexStream(List<UIVertex> verts, List<Vector3> positions, List<Color32> colors,
        List<Vector4> uv0S, List<Vector4> uv1S, List<Vector4> uv2S, List<Vector4> uv3S, List<Vector3> normals,
        List<Vector4> tangents)
    {
        ArgumentNullException.ThrowIfNull(verts);
        ArgumentNullException.ThrowIfNull(positions);
        var count = positions.Count;
        CheckChannel(colors, count, nameof(colors)); CheckChannel(uv0S, count, nameof(uv0S));
        CheckChannel(uv1S, count, nameof(uv1S)); CheckChannel(uv2S, count, nameof(uv2S));
        CheckChannel(uv3S, count, nameof(uv3S)); CheckChannel(normals, count, nameof(normals));
        CheckChannel(tangents, count, nameof(tangents));
        for (var i = 0; i < count; i++)
        {
            var v = UIVertex.simpleVert;
            v.position = positions[i];
            if (colors is { Count: > 0 }) v.color = colors[i];
            if (uv0S is { Count: > 0 }) v.uv0 = uv0S[i];
            if (uv1S is { Count: > 0 }) v.uv1 = uv1S[i];
            if (uv2S is { Count: > 0 }) v.uv2 = uv2S[i];
            if (uv3S is { Count: > 0 }) v.uv3 = uv3S[i];
            if (normals is { Count: > 0 }) v.normal = normals[i];
            if (tangents is { Count: > 0 }) v.tangent = tangents[i];
            verts.Add(v);
        }
    }

    private static void CheckChannel<T>(List<T>? channel, int count, string name)
    {
        if (channel is { Count: > 0 } && channel.Count != count)
            throw new ArgumentException($"{name} has {channel.Count} entries and the positions have {count}", name);
    }
}
