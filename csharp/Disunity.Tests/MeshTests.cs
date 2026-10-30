using UnityEngine;

/// <summary>Mesh: the CPU-side vertex channels, sub-meshes and indices, bounds, recalculation and readability.</summary>
internal static partial class MeshTests
{
    public static void Run()
    {
        PositionsRoundTrip();
        ChannelRoundTrips();
        ColorsAndColors32();
        UvChannelsAndDimensions();
        ChannelLengthValidation();
        NullClearsChannels();
        Console.WriteLine("PASS: Mesh channels, colors, uv dimensions and channel validation");
        SubMeshCounts();
        SubMeshQueries();
        TrianglesRules();
        BaseVertexRange();
        IndexFormatLimit();
        TopologiesAndSetIndices();
        VertexCountChanges();
        BoundsRules();
        Console.WriteLine("PASS: Mesh sub-meshes, indices, topologies, index format limit, vertex count changes and bounds");
        NormalsOfAQuad();
        NormalsAreAreaWeighted();
        TangentsOfAQuad();
        TangentsNeedNormalsAndUvs();
        ClearEmptiesTheMesh();
        UnreadableAfterUpload();
        OptimizeKeepsData();
        SnapshotIsACopy();
        Console.WriteLine("PASS: Mesh normals, tangents, Clear, readability, Optimize and the renderer snapshot");
    }

    private static Vector3[] QuadPositions() => [new(0, 0, 0), new(0, 1, 0), new(1, 1, 0), new(1, 0, 0)];

    private static void PositionsRoundTrip()
    {
        var mesh = new Mesh();
        Expect.Equal(0, mesh.vertexCount, "an empty mesh has no vertices");
        var source = QuadPositions();
        mesh.vertices = source;
        source[0] = new Vector3(9, 9, 9);
        Expect.Equal(4, mesh.vertexCount, "vertexCount is the positions count");
        Expect.Sequence(QuadPositions(), mesh.vertices, "the setter copied the array");
        var copy = mesh.vertices;
        copy[1] = new Vector3(7, 7, 7);
        Expect.Sequence(QuadPositions(), mesh.vertices, "the getter returns a copy");
        var list = new List<Vector3> { new(5, 5, 5), new(6, 6, 6), new(7, 7, 7), new(8, 8, 8), new(9, 9, 9) };
        mesh.SetVertices(list);
        list.Clear();
        Expect.Equal(5, mesh.vertexCount, "SetVertices(List) copies the list");
        var filled = new List<Vector3> { new(1, 1, 1) };
        mesh.GetVertices(filled);
        Expect.Sequence(new[] { new Vector3(5, 5, 5), new(6, 6, 6), new(7, 7, 7), new(8, 8, 8), new(9, 9, 9) }, filled, "GetVertices clears the list first");
        mesh.SetVertices(QuadPositions());
        Expect.Equal(4, mesh.vertexCount, "SetVertices(array)");
    }

    private static void ChannelRoundTrips()
    {
        var mesh = new Mesh { vertices = QuadPositions() };
        var normals = new[] { Vector3.forward, Vector3.back, Vector3.up, Vector3.down };
        mesh.normals = normals;
        Expect.Sequence(normals, mesh.normals, "normals property");
        mesh.SetNormals(new List<Vector3>(normals.Reverse()));
        var read = new List<Vector3> { Vector3.one };
        mesh.GetNormals(read);
        Expect.Sequence(normals.Reverse(), read, "SetNormals(List) then GetNormals");
        mesh.SetNormals(normals);
        Expect.Sequence(normals, mesh.normals, "SetNormals(array)");

        var tangents = new[] { new Vector4(1, 0, 0, 1), new(0, 1, 0, -1), new(0, 0, 1, 1), new(1, 1, 0, -1) };
        mesh.tangents = tangents;
        Expect.Sequence(tangents, mesh.tangents, "tangents property");
        mesh.SetTangents(new List<Vector4>(tangents.Reverse()));
        var tangentRead = new List<Vector4>();
        mesh.GetTangents(tangentRead);
        Expect.Sequence(tangents.Reverse(), tangentRead, "SetTangents(List) then GetTangents");
        mesh.SetTangents(tangents);
        Expect.Sequence(tangents, mesh.tangents, "SetTangents(array)");
        Expect.Equal(0, new Mesh().normals.Length + new Mesh().tangents.Length + new Mesh().colors.Length + new Mesh().uv.Length, "unset channels are empty");
    }

    private static void ColorsAndColors32()
    {
        var mesh = new Mesh { vertices = QuadPositions() };
        var colors = new[] { new Color(1f, 0.5f, 0f, 1f), Color.red, new Color(0.2f, 0.4f, 0.6f, 0.8f), Color.clear };
        mesh.colors = colors;
        Expect.Sequence(colors, mesh.colors, "colors keep full float precision");
        var packed = mesh.colors32;
        Expect.Equal(new Color32(255, 128, 0, 255), packed[0], "colors32 converts from colors");
        Expect.Equal(new Color32(255, 0, 0, 255), packed[1], "colors32 of red");
        Expect.Equal(new Color32(51, 102, 153, 204), packed[2], "colors32 rounds to the nearest byte");
        var bytes = new[] { new Color32(255, 0, 51, 255), new(0, 255, 0, 128), new(1, 2, 3, 4), new(250, 251, 252, 253) };
        mesh.colors32 = bytes;
        Expect.Sequence(bytes, mesh.colors32, "colors32 round trip");
        Expect.Equal(new Color(0f, 1f, 0f, 128 / 255f), mesh.colors[1], "colors converts from colors32");
        mesh.SetColors(new List<Color32>(bytes.Reverse()));
        var floats = new List<Color>();
        mesh.GetColors(floats);
        Expect.Sequence(bytes.Reverse().Select(c => (Color)c), floats, "SetColors(List<Color32>) then GetColors(List<Color>)");
        var bytesBack = new List<Color32> { default };
        mesh.GetColors(bytesBack);
        Expect.Sequence(bytes.Reverse(), bytesBack, "GetColors(List<Color32>)");
        mesh.SetColors(new List<Color>(colors));
        Expect.Sequence(colors, mesh.colors, "SetColors(List<Color>)");
        mesh.SetColors(bytes);
        Expect.Sequence(bytes, mesh.colors32, "SetColors(Color32[])");
        mesh.SetColors(colors);
        Expect.Sequence(colors, mesh.colors, "SetColors(Color[])");
    }

    private static void UvChannelsAndDimensions()
    {
        var mesh = new Mesh { vertices = QuadPositions() };
        var two = new[] { new Vector2(0, 0), new(0, 1), new(1, 1), new(1, 0) };
        mesh.uv = two;
        Expect.Sequence(two, mesh.uv, "uv round trip");
        var wide = new List<Vector4> { Vector4.one };
        mesh.GetUVs(0, wide);
        Expect.Sequence(two.Select(v => new Vector4(v.x, v.y, 0, 0)), wide, "Vector2 set then Vector4 get pads zeros");
        var four = new List<Vector4> { new(1, 2, 3, 4), new(5, 6, 7, 8), new(9, 10, 11, 12), new(13, 14, 15, 16) };
        mesh.SetUVs(1, four);
        var narrow = new List<Vector2> { Vector2.one };
        mesh.GetUVs(1, narrow);
        Expect.Sequence(new[] { new Vector2(1, 2), new(5, 6), new(9, 10), new(13, 14) }, narrow, "Vector4 set then Vector2 get drops z and w");
        var three = new List<Vector3>();
        mesh.GetUVs(1, three);
        Expect.Equal(new Vector3(9, 10, 11), three[2], "Vector4 set then Vector3 get drops w");
        Expect.Sequence(narrow, mesh.uv2, "uv2 is channel 1");
        mesh.GetUVs(1, wide);
        Expect.Sequence(four, wide, "Vector4 set then Vector4 get is exact");
        mesh.SetUVs(2, new[] { new Vector3(1, 2, 3), new(4, 5, 6), new(7, 8, 9), new(0, 0, 1) });
        Expect.Equal(new Vector2(4, 5), mesh.uv3[1], "uv3 is channel 2");
        mesh.SetUVs(3, new[] { new Vector4(1, 2, 3, 4), new(1, 1, 1, 1), new(2, 2, 2, 2), new(3, 3, 3, 3) });
        mesh.SetUVs(4, two);
        Expect.Equal(new Vector2(1, 1), mesh.uv5[2], "SetUVs(Vector2[]) writes channel 4");
        mesh.SetUVs(5, new List<Vector2>(two));
        mesh.SetUVs(6, new List<Vector3>(three.Take(0)));
        mesh.uv8 = two;
        Expect.Sequence(two, mesh.uv8, "uv8 is channel 7");
        var channel7 = new List<Vector2>();
        mesh.GetUVs(7, channel7);
        Expect.Sequence(two, channel7, "GetUVs(7)");
        mesh.uv4 = two;
        mesh.uv6 = two;
        mesh.uv7 = two;
        Expect.Sequence(two, mesh.uv4, "uv4");
        Expect.Throws<ArgumentOutOfRangeException>(() => mesh.SetUVs(-1, two), "SetUVs channel -1");
        Expect.Throws<ArgumentOutOfRangeException>(() => mesh.SetUVs(8, two), "SetUVs channel 8");
        Expect.Throws<ArgumentOutOfRangeException>(() => mesh.GetUVs(8, narrow), "GetUVs channel 8");
        var cleared = new List<Vector2> { Vector2.one };
        new Mesh().GetUVs(0, cleared);
        Expect.Equal(0, cleared.Count, "an unset channel reads as empty");
    }

    private static void ChannelLengthValidation()
    {
        var mesh = new Mesh { vertices = QuadPositions() };
        var channels = new (string Name, Action Set)[]
        {
            ("normals", () => mesh.normals = new Vector3[3]),
            ("tangents", () => mesh.tangents = new Vector4[5]),
            ("colors", () => mesh.colors = new Color[1]),
            ("colors32", () => mesh.colors32 = new Color32[2]),
            ("uv", () => mesh.uv = new Vector2[3]),
            ("uv2", () => mesh.SetUVs(1, new List<Vector4>(new Vector4[2]))),
            ("uv3", () => mesh.uv3 = new Vector2[1]),
            ("uv8", () => mesh.SetUVs(7, new Vector3[6])),
        };
        foreach (var (name, set) in channels)
        {
            var error = Expect.Catch<ArgumentException>(set, $"{name} with the wrong length");
            Expect.That(error.Message.Contains(name + " ", StringComparison.Ordinal) || error.Message.Contains(name + ":", StringComparison.Ordinal), $"the message names {name}: {error.Message}");
        }
        mesh.normals = new[] { Vector3.up, Vector3.up, Vector3.up, Vector3.up };
        Expect.Throws<ArgumentException>(() => mesh.normals = new Vector3[2], "a failed set");
        Expect.Equal(4, mesh.normals.Length, "a failed set leaves the channel unchanged");
        mesh.normals = new Vector3[0];
        Expect.Equal(0, mesh.normals.Length, "length 0 clears the channel");
        Expect.Throws<ArgumentException>(() => new Mesh().normals = new Vector3[1], "a channel before any position");
    }

    private static void NullClearsChannels()
    {
        var mesh = new Mesh { vertices = QuadPositions() };
        mesh.uv = new Vector2[4];
        mesh.colors = new Color[4];
        mesh.uv = null!;
        mesh.colors = null!;
        mesh.SetNormals((List<Vector3>)null!);
        Expect.Equal(0, mesh.uv.Length + mesh.colors.Length + mesh.normals.Length, "null clears a channel");
        mesh.vertices = null!;
        Expect.Equal(0, mesh.vertexCount, "null positions clear the positions");
    }
}
