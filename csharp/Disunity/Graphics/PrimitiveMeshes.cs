using Disunity.Hosting;

namespace UnityEngine;

/// <summary>
/// The meshes <see cref="GameObject.CreatePrimitive"/> uses, built once and shared by every object of that type
/// (the engine's choice). Triangles wind clockwise seen from the front, so the geometric normal of triangle (a, b, c),
/// the cross product of (b - a) and (c - a), points the way the vertex normals do. Every vertex has a normal, a tangent
/// along the first uv direction, and uvs from 0 to 1. A shared mesh that a game destroys is built again on next use.
/// </summary>
internal static class PrimitiveMeshes
{
    /// <summary>Steps around the axis of the sphere, capsule and cylinder (the engine's choice).</summary>
    private const int Segments = 24;

    /// <summary>Latitude steps of the sphere, and of each hemisphere of the capsule.</summary>
    private const int Rings = 16;

    private static readonly Dictionary<PrimitiveType, Mesh> Shared = [];

    /// <summary>One point of a surface of revolution: its distance from the axis, height, normal in the same two
    /// coordinates, and the uv v value.</summary>
    private readonly record struct Ring(float Radius, float Height, float NormalRadius, float NormalHeight, float V);

    internal static Mesh Of(PrimitiveType type)
    {
        DisunityHost.RequireMainThread("GameObject.CreatePrimitive");
        if (!Enum.IsDefined(type)) throw new ArgumentOutOfRangeException(nameof(type), type, $"{(int)type} is not a PrimitiveType");
        if (Shared.TryGetValue(type, out var mesh) && mesh) return mesh;
        return Shared[type] = Build(type);
    }

    private static Mesh Build(PrimitiveType type)
    {
        var shape = new MeshShape();
        switch (type)
        {
            case PrimitiveType.Quad: shape.Grid(Vector3.zero, new Vector3(0, 0, -1), Vector3.right, Vector3.up, 1, 1); break;
            case PrimitiveType.Plane: shape.Grid(Vector3.zero, Vector3.up, Vector3.right, Vector3.forward, 10, 10); break;
            case PrimitiveType.Cube: AddCube(shape); break;
            case PrimitiveType.Sphere: shape.Revolve(SphereProfile()); break;
            case PrimitiveType.Capsule: shape.Revolve(CapsuleProfile()); break;
            default: AddCylinder(shape); break;
        }
        return shape.ToMesh(type.ToString());
    }

    /// <summary>Six faces of one quad each. For each face the cross product of the two edge directions is the
    /// reverse of the normal, which is what makes the winding clockwise from outside.</summary>
    private static void AddCube(MeshShape shape)
    {
        (Vector3 Normal, Vector3 U, Vector3 V)[] faces =
        [
            (new(0, 0, -1), Vector3.right, Vector3.up), (new(0, 0, 1), Vector3.left, Vector3.up),
            (Vector3.right, Vector3.forward, Vector3.up), (Vector3.left, Vector3.back, Vector3.up),
            (Vector3.up, Vector3.right, Vector3.forward), (Vector3.down, Vector3.right, Vector3.back),
        ];
        foreach (var (normal, u, v) in faces) shape.Grid(normal * 0.5f, normal, u, v, 1, 1);
    }

    private static Ring[] SphereProfile() => Hemisphere(-MathF.PI / 2, MathF.PI / 2, Rings, 0f, 0.5f, 1f);

    private static Ring[] CapsuleProfile()
    {
        var bottom = Hemisphere(-MathF.PI / 2, 0f, Rings / 2, -0.5f, 0.5f, 0.5f);
        var top = Hemisphere(0f, MathF.PI / 2, Rings / 2, 0.5f, 0.5f, 0.5f);
        return [.. bottom, .. top.Select(ring => ring with { V = 0.5f + ring.V })];
    }

    /// <summary>The rings of a sphere of <paramref name="radius"/> centered at <paramref name="center"/> between two
    /// latitudes, from <paramref name="from"/> to <paramref name="to"/> radians; v runs from 0 to 1 over them.
    /// A ring at a pole has exactly zero radius.</summary>
    private static Ring[] Hemisphere(float from, float to, int steps, float center, float radius, float span)
    {
        var rings = new Ring[steps + 1];
        for (var step = 0; step <= steps; step++)
        {
            var latitude = from + (to - from) * step / steps;
            var (sin, cos) = (MathF.Sin(latitude), MathF.Cos(latitude));
            var atPole = MathF.Abs(latitude) >= MathF.PI / 2 - 1e-6f;
            rings[step] = new Ring(atPole ? 0f : radius * cos, center + radius * sin, cos, sin, span * step / steps);
        }
        return rings;
    }

    /// <summary>A flat bottom cap, the side, and a flat top cap, each with its own vertices so the edges stay sharp.</summary>
    private static void AddCylinder(MeshShape shape)
    {
        shape.Revolve([new Ring(0, -1, 0, -1, 0), new Ring(0.5f, -1, 0, -1, 1)]);
        shape.Revolve([new Ring(0.5f, -1, 1, 0, 0), new Ring(0.5f, 1, 1, 0, 1)]);
        shape.Revolve([new Ring(0.5f, 1, 0, 1, 0), new Ring(0, 1, 0, 1, 1)]);
    }

    /// <summary>Vertex and index lists for one primitive.</summary>
    private sealed class MeshShape
    {
        private readonly List<Vector3> _positions = [];
        private readonly List<Vector3> _normals = [];
        private readonly List<Vector4> _tangents = [];
        private readonly List<Vector2> _uvs = [];
        private readonly List<int> _indices = [];

        private void Vertex(Vector3 position, Vector3 normal, Vector4 tangent, Vector2 uv)
        {
            _positions.Add(position);
            _normals.Add(normal);
            _tangents.Add(tangent);
            _uvs.Add(uv);
        }

        /// <summary>The quad (a, b, c, d) as triangles (a, b, c) and (b, d, c), where b is above a and c is right of a.</summary>
        private void Cell(int a, int b, int c, int d) => _indices.AddRange([a, b, c, b, d, c]);

        /// <summary>A flat grid of <paramref name="cells"/> x <paramref name="cells"/> squares spanning
        /// <paramref name="size"/> along <paramref name="u"/> and <paramref name="v"/> around <paramref name="center"/>.</summary>
        internal void Grid(Vector3 center, Vector3 normal, Vector3 u, Vector3 v, int cells, float size)
        {
            var first = _positions.Count;
            var tangent = new Vector4(u.x, u.y, u.z, -1f);
            for (var row = 0; row <= cells; row++)
                for (var column = 0; column <= cells; column++)
                {
                    var (s, t) = ((float)column / cells, (float)row / cells);
                    Vertex(center + u * ((s - 0.5f) * size) + v * ((t - 0.5f) * size), normal, tangent, new Vector2(s, t));
                }
            for (var row = 0; row < cells; row++)
                for (var column = 0; column < cells; column++)
                {
                    var a = first + row * (cells + 1) + column;
                    Cell(a, a + cells + 1, a + 1, a + cells + 2);
                }
        }

        /// <summary>Sweeps a profile around the Y axis. A triangle that would collapse onto the axis is left out.</summary>
        internal void Revolve(Ring[] profile)
        {
            var first = _positions.Count;
            foreach (var ring in profile)
                for (var step = 0; step <= Segments; step++)
                {
                    var angle = 2 * MathF.PI * step / Segments;
                    var (sin, cos) = (MathF.Sin(angle), MathF.Cos(angle));
                    Vertex(new Vector3(ring.Radius * cos, ring.Height, ring.Radius * sin), new Vector3(ring.NormalRadius * cos, ring.NormalHeight, ring.NormalRadius * sin),
                        new Vector4(-sin, 0, cos, -1f), new Vector2((float)step / Segments, ring.V));
                }
            for (var row = 0; row + 1 < profile.Length; row++)
                for (var step = 0; step < Segments; step++)
                {
                    int a = first + row * (Segments + 1) + step, b = a + Segments + 1;
                    if (profile[row].Radius > 0) _indices.AddRange([a, b, a + 1]);
                    if (profile[row + 1].Radius > 0) _indices.AddRange([b, b + 1, a + 1]);
                }
        }

        internal Mesh ToMesh(string name)
        {
            var mesh = new Mesh { name = name, hideFlags = HideFlags.HideAndDontSave };
            mesh.SetVertices(_positions);
            mesh.SetNormals(_normals);
            mesh.SetTangents(_tangents);
            mesh.SetUVs(0, _uvs);
            mesh.SetTriangles(_indices, 0);
            mesh.RecalculateBounds();
            return mesh;
        }
    }
}
