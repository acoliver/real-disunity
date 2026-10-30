using UnityEngine;
using static GraphicsTestSupport;

/// <summary>The three UIVertex stream conversions on CanvasRenderer.</summary>
internal static class CanvasRendererStreamTests
{
    public static void Run()
    {
        SplitThenCreateRoundTrips();
        Console.WriteLine("PASS: CanvasRenderer.CreateUIVertexStream then SplitUIVertexStreams round trips every channel");
        CreateExpandsIndices();
        Console.WriteLine("PASS: CanvasRenderer.CreateUIVertexStream emits one vertex per index and rejects a bad index");
        AddAppends();
        Console.WriteLine("PASS: CanvasRenderer.AddUIVertexStream appends, fills absent channels and rejects a short channel");
    }

    private static UIVertex Vertex(int i) => new()
    {
        position = new Vector3(i, i + 0.5f, -i),
        normal = new Vector3(0f, i, 1f),
        tangent = new Vector4(i, 1f, 2f, -1f),
        color = new Color32((byte)(i * 10), (byte)(i * 20), (byte)(i * 30), 255),
        uv0 = new Vector4(i, 1f, 2f, 3f),
        uv1 = new Vector4(4f, i, 5f, 6f),
        uv2 = new Vector4(7f, 8f, i, 9f),
        uv3 = new Vector4(10f, 11f, 12f, i),
    };

    private sealed class Streams
    {
        public readonly List<Vector3> Positions = new();
        public readonly List<Color32> Colors = new();
        public readonly List<Vector4> Uv0 = new(), Uv1 = new(), Uv2 = new(), Uv3 = new(), Tangents = new();
        public readonly List<Vector3> Normals = new();
        public readonly List<int> Indices = new();
    }

    private static void Split(List<UIVertex> verts, Streams s) => CanvasRenderer.SplitUIVertexStreams(verts, s.Positions, s.Colors,
        s.Uv0, s.Uv1, s.Uv2, s.Uv3, s.Normals, s.Tangents, s.Indices);

    private static void Create(List<UIVertex> verts, Streams s) => CanvasRenderer.CreateUIVertexStream(verts, s.Positions, s.Colors,
        s.Uv0, s.Uv1, s.Uv2, s.Uv3, s.Normals, s.Tangents, s.Indices);

    private static bool Same(UIVertex a, UIVertex b) => a.position == b.position && a.normal == b.normal && a.tangent == b.tangent
        && a.color.Equals(b.color) && a.uv0 == b.uv0 && a.uv1 == b.uv1 && a.uv2 == b.uv2 && a.uv3 == b.uv3;

    private static void SplitThenCreateRoundTrips()
    {
        var triangles = new List<UIVertex> { Vertex(1), Vertex(2), Vertex(3), Vertex(3), Vertex(2), Vertex(4) };
        var s = new Streams();
        s.Positions.Add(Vector3.one);
        s.Indices.Add(99);
        Split(triangles, s);
        Ensure(s.Positions.Count == 6 && s.Indices.Count == 6 && s.Tangents.Count == 6, "one entry per vertex, and the lists are cleared first");
        Ensure(s.Indices.SequenceEqual([0, 1, 2, 3, 4, 5]), "the indices count the stream");
        Ensure(s.Positions[5] == Vertex(4).position && s.Normals[2] == Vertex(3).normal && s.Uv3[1] == Vertex(2).uv3 && s.Uv1[0] == Vertex(1).uv1, "channels follow the vertices");
        var back = new List<UIVertex> { Vertex(9) };
        Create(back, s);
        Ensure(back.Count == 6 && back.Zip(triangles).All(pair => Same(pair.First, pair.Second)), "Create restores the stream");
    }

    private static void CreateExpandsIndices()
    {
        var s = new Streams();
        for (var i = 0; i < 3; i++) s.Positions.Add(Vertex(i).position);
        s.Indices.AddRange([2, 0, 2, 1]);
        var verts = new List<UIVertex> { Vertex(9) };
        Create(verts, s);
        Ensure(verts.Count == 4 && verts[0].position == Vertex(2).position && verts[1].position == Vertex(0).position && verts[2].position == Vertex(2).position, "a vertex repeats for each index that names it");
        Ensure(Same(verts[0], MixedWithDefaults(Vertex(2).position)), "absent channels take the simple vertex values");
        s.Indices.Add(3);
        Throws<ArgumentOutOfRangeException>(() => Create(verts, s), "an index past the vertices", "index");
        s.Indices[^1] = -1;
        Throws<ArgumentOutOfRangeException>(() => Create(verts, s), "a negative index", "index");
        Throws<ArgumentNullException>(() => CanvasRenderer.CreateUIVertexStream(null!, s.Positions, s.Colors, s.Uv0, s.Uv1, s.Uv2, s.Uv3, s.Normals, s.Tangents, s.Indices), "null verts");
    }

    private static UIVertex MixedWithDefaults(Vector3 position)
    {
        var v = UIVertex.simpleVert;
        v.position = position;
        return v;
    }

    private static void AddAppends()
    {
        var s = new Streams();
        for (var i = 1; i <= 2; i++)
        {
            var v = Vertex(i);
            s.Positions.Add(v.position); s.Colors.Add(v.color); s.Uv0.Add(v.uv0); s.Uv1.Add(v.uv1); s.Uv2.Add(v.uv2);
            s.Uv3.Add(v.uv3); s.Normals.Add(v.normal); s.Tangents.Add(v.tangent);
        }
        var verts = new List<UIVertex> { Vertex(7) };
        CanvasRenderer.AddUIVertexStream(verts, s.Positions, s.Colors, s.Uv0, s.Uv1, s.Uv2, s.Uv3, s.Normals, s.Tangents);
        Ensure(verts.Count == 3 && Same(verts[0], Vertex(7)) && Same(verts[1], Vertex(1)) && Same(verts[2], Vertex(2)), "added after the existing vertex, in order");
        CanvasRenderer.AddUIVertexStream(verts, s.Positions, null!, null!, null!, null!, null!, null!, null!);
        Ensure(verts.Count == 5 && Same(verts[3], MixedWithDefaults(Vertex(1).position)), "null channels are absent");
        s.Colors.RemoveAt(1);
        Throws<ArgumentException>(() => CanvasRenderer.AddUIVertexStream(verts, s.Positions, s.Colors, s.Uv0, s.Uv1, s.Uv2, s.Uv3, s.Normals, s.Tangents), "a channel shorter than the positions", "colors");
        Ensure(verts.Count == 5, "a rejected call appends nothing");
        Throws<ArgumentNullException>(() => CanvasRenderer.AddUIVertexStream(verts, null!, null!, null!, null!, null!, null!, null!, null!), "null positions");
    }
}
