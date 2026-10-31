using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>GameObject.CreatePrimitive: each primitive's mesh data and bounds, the front-face winding, the shared meshes,
/// and the components the new object gets.</summary>
internal static class PrimitiveTests
{
    private readonly record struct Shape(PrimitiveType Type, int Vertices, int Triangles, Vector3 Size);

    private static readonly Shape[] Shapes =
    [
        new(PrimitiveType.Quad, 4, 2, new Vector3(1, 1, 0)),
        new(PrimitiveType.Plane, 121, 200, new Vector3(10, 0, 10)),
        new(PrimitiveType.Cube, 24, 12, new Vector3(1, 1, 1)),
        new(PrimitiveType.Sphere, 425, 720, new Vector3(1, 1, 1)),
        new(PrimitiveType.Capsule, 450, 768, new Vector3(1, 2, 1)),
        new(PrimitiveType.Cylinder, 150, 96, new Vector3(1, 2, 1)),
    ];

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            EveryPrimitiveHasItsCounts();
            EveryChannelIsWellFormed();
            TrianglesWindClockwiseFromTheFront();
            TheQuadIsExact();
            ThePlaneAndCubeLayouts();
            RoundShapesHaveTheirRadius();
            MeshesAreSharedAndOutliveTheirObjects();
            ObjectsGetTheirComponents();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: CreatePrimitive builds the six shapes with counts, channels, winding, shared meshes and components");
    }

    private static Mesh MeshOf(PrimitiveType type) => GameObject.CreatePrimitive(type).GetComponent<MeshFilter>()!.sharedMesh!;

    private static void Near(Vector3 expected, Vector3 actual, string what) =>
        Ensure((expected - actual).magnitude <= 1e-5f, $"{what}: expected {expected}, got {actual}");

    private static void EveryPrimitiveHasItsCounts()
    {
        foreach (var shape in Shapes)
        {
            var mesh = MeshOf(shape.Type);
            Ensure(mesh.name == shape.Type.ToString(), $"{shape.Type} mesh is named {mesh.name}");
            Ensure(mesh.vertexCount == shape.Vertices, $"{shape.Type} has {mesh.vertexCount} vertices, expected {shape.Vertices}");
            Ensure(mesh.triangles.Length == shape.Triangles * 3 && mesh.subMeshCount == 1, $"{shape.Type} has {mesh.triangles.Length / 3} triangles in {mesh.subMeshCount} sub-meshes, expected {shape.Triangles} in 1");
            Near(Vector3.zero, mesh.bounds.center, $"{shape.Type} bounds center");
            Near(shape.Size, mesh.bounds.size, $"{shape.Type} bounds size");
            Ensure(mesh.isReadable, $"{shape.Type} is readable");
        }
    }

    private static void EveryChannelIsWellFormed()
    {
        foreach (var shape in Shapes)
        {
            var mesh = MeshOf(shape.Type);
            Ensure(mesh.normals.Length == shape.Vertices && mesh.tangents.Length == shape.Vertices && mesh.uv.Length == shape.Vertices,
                $"{shape.Type} has a normal, tangent and uv for each vertex");
            Ensure(mesh.normals.All(normal => MathF.Abs(normal.magnitude - 1f) < 1e-4f), $"{shape.Type} normals have unit length");
            Ensure(mesh.uv.All(uv => uv.x >= 0 && uv.x <= 1 && uv.y >= 0 && uv.y <= 1), $"{shape.Type} uvs lie in 0 to 1");
            Ensure(mesh.triangles.All(index => index >= 0 && index < shape.Vertices), $"{shape.Type} indices name vertices");
            var half = shape.Size * 0.5f;
            Ensure(mesh.vertices.All(v => MathF.Abs(v.x) <= half.x + 1e-5f && MathF.Abs(v.y) <= half.y + 1e-5f && MathF.Abs(v.z) <= half.z + 1e-5f),
                $"{shape.Type} vertices lie inside its bounds");
        }
    }

    private static void TrianglesWindClockwiseFromTheFront()
    {
        foreach (var shape in Shapes)
        {
            var mesh = MeshOf(shape.Type);
            var (positions, normals, indices) = (mesh.vertices, mesh.normals, mesh.triangles);
            var faced = 0;
            for (var i = 0; i < indices.Length; i += 3)
            {
                int a = indices[i], b = indices[i + 1], c = indices[i + 2];
                var facing = Vector3.Cross(positions[b] - positions[a], positions[c] - positions[a]);
                Ensure(facing.magnitude > 1e-6f, $"{shape.Type} triangle {i / 3} has area");
                Ensure(Vector3.Dot(facing, normals[a] + normals[b] + normals[c]) > 0, $"{shape.Type} triangle {i / 3} faces away from its normals");
                faced++;
            }
            Ensure(faced == shape.Triangles, $"{shape.Type}: {faced} triangles checked");
        }
    }

    private static void TheQuadIsExact()
    {
        var mesh = MeshOf(PrimitiveType.Quad);
        Vector3[] corners = [new(-0.5f, -0.5f, 0), new(0.5f, -0.5f, 0), new(-0.5f, 0.5f, 0), new(0.5f, 0.5f, 0)];
        Expect.Sequence(corners, mesh.vertices, "the quad's vertices run bottom left, bottom right, top left, top right");
        Expect.Sequence(new Vector2[] { new(0, 0), new(1, 0), new(0, 1), new(1, 1) }, mesh.uv, "the quad's uvs cover 0 to 1");
        Expect.Sequence(Enumerable.Repeat(new Vector3(0, 0, -1), 4), mesh.normals, "the quad faces back along -Z");
        Expect.Sequence(Enumerable.Repeat(new Vector4(1, 0, 0, -1), 4), mesh.tangents, "the quad's tangents follow +X");
        Expect.Sequence(new[] { 0, 2, 1, 2, 3, 1 }, mesh.triangles, "the quad's two triangles");
    }

    private static void ThePlaneAndCubeLayouts()
    {
        var plane = MeshOf(PrimitiveType.Plane);
        Near(new Vector3(-5, 0, -5), plane.vertices[0], "the plane's first vertex");
        Near(new Vector3(5, 0, -5), plane.vertices[10], "the plane's first row ends at +X");
        Near(new Vector3(5, 0, 5), plane.vertices[120], "the plane's last vertex");
        Ensure(plane.normals.All(normal => normal == Vector3.up), "the plane faces up");
        Near(new Vector3(0.1f, 0.2f, 0), plane.uv.Select(uv => (Vector3)uv).ElementAt(23), "the plane's uvs step by a tenth");
        var cube = MeshOf(PrimitiveType.Cube);
        var faces = cube.normals.GroupBy(normal => normal).ToList();
        Ensure(faces.Count == 6 && faces.All(face => face.Count() == 4), "the cube has six faces of four vertices with their own normals");
        Ensure(cube.vertices.All(v => MathF.Abs(MathF.Abs(v.x) - 0.5f) < 1e-6f && MathF.Abs(MathF.Abs(v.y) - 0.5f) < 1e-6f && MathF.Abs(MathF.Abs(v.z) - 0.5f) < 1e-6f),
            "every cube vertex is a corner of the unit cube");
    }

    private static void RoundShapesHaveTheirRadius()
    {
        var sphere = MeshOf(PrimitiveType.Sphere);
        Ensure(sphere.vertices.All(v => MathF.Abs(v.magnitude - 0.5f) < 1e-5f), "sphere vertices lie at radius 0.5");
        Ensure(sphere.vertices.Zip(sphere.normals).All(pair => Vector3.Dot(pair.First.normalized, pair.Second) > 0.9999f), "sphere normals point away from the center");
        var capsule = MeshOf(PrimitiveType.Capsule);
        Ensure(capsule.vertices.All(v => Mathf.Sqrt(v.x * v.x + v.z * v.z) <= 0.5f + 1e-5f), "capsule vertices lie within radius 0.5");
        Ensure(capsule.vertices.Any(v => MathF.Abs(v.y - 1f) < 1e-5f) && capsule.vertices.Any(v => MathF.Abs(v.y + 1f) < 1e-5f), "the capsule is 2 tall");
        var heights = capsule.vertices.Zip(capsule.uv).OrderBy(pair => pair.First.y).ToArray();
        Ensure(heights.Zip(heights.Skip(1)).All(pair => pair.Second.Second.y >= pair.First.Second.y - 1e-5f), "capsule uv v rises with height over both halves");
        Ensure(MathF.Abs(heights[0].Second.y) < 1e-5f && MathF.Abs(heights[^1].Second.y - 1f) < 1e-5f, "capsule uv v runs from 0 at the bottom to 1 at the top");
        var cylinder = MeshOf(PrimitiveType.Cylinder);
        Ensure(cylinder.normals.Count(normal => normal == Vector3.up) == 50 && cylinder.normals.Count(normal => normal == Vector3.down) == 50,
            "the cylinder has flat top and bottom caps with their own normals");
    }

    private static void MeshesAreSharedAndOutliveTheirObjects()
    {
        var first = GameObject.CreatePrimitive(PrimitiveType.Cube);
        var second = GameObject.CreatePrimitive(PrimitiveType.Cube);
        var mesh = first.GetComponent<MeshFilter>()!.sharedMesh!;
        Ensure(ReferenceEquals(mesh, second.GetComponent<MeshFilter>()!.sharedMesh), "two cubes share one mesh");
        Ensure(!ReferenceEquals(mesh, MeshOf(PrimitiveType.Sphere)), "different primitives have different meshes");
        UnityEngine.Object.DestroyImmediate(first);
        UnityEngine.Object.DestroyImmediate(second);
        Ensure(mesh && mesh.vertexCount == 24, "the shared mesh survives the destruction of the objects that use it");
        Ensure(ReferenceEquals(mesh, MeshOf(PrimitiveType.Cube)), "a later cube still shares that mesh");
        UnityEngine.Object.DestroyImmediate(mesh);
        var rebuilt = MeshOf(PrimitiveType.Cube);
        Ensure(rebuilt && !ReferenceEquals(rebuilt, mesh) && rebuilt.vertexCount == 24, "a destroyed shared mesh is built again");
    }

    private static void ObjectsGetTheirComponents()
    {
        foreach (var shape in Shapes)
        {
            var go = GameObject.CreatePrimitive(shape.Type);
            Ensure(go.name == shape.Type.ToString() && go.activeSelf, $"the {shape.Type} object is named after its type and active");
            Ensure(go.GetComponent<MeshFilter>() is not null && go.GetComponent<MeshRenderer>() is not null, $"the {shape.Type} object has a mesh filter and renderer");
            Ensure(go.transform.position == Vector3.zero && go.transform.localScale == Vector3.one, $"the {shape.Type} object sits at the origin");
            Ensure(go.GetComponent<MeshRenderer>()!.sharedMaterial is null, $"the {shape.Type} object has no material, since the engine has no default one");
        }
        Throws<NotSupportedException>(() => GameObject.CreatePrimitive(PrimitiveType.Cube).AddComponent<BoxCollider>(),
            "attaching a 3D collider, which is why primitives get none", "3D physics components");
        Throws<ArgumentOutOfRangeException>(() => GameObject.CreatePrimitive((PrimitiveType)99), "an undefined primitive type", "99");
    }
}
