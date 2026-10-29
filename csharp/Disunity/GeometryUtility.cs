namespace UnityEngine;

/// <summary>
/// Frustum, bounds and polygon helpers. Frustum planes come from the rows of the matrix
/// (Gribb and Hartmann): for a point in clip space, -w &lt;= x, y, z &lt;= w, each side adds or subtracts a row
/// from the fourth row. The planes are in Unity's order (left, right, bottom, top, near, far), their normals
/// point into the frustum and each is normalized. A camera's planes come from its projection matrix times
/// its world-to-camera matrix.
/// </summary>
public sealed class GeometryUtility
{
    public GeometryUtility() { }

    private const int PlaneCount = 6;
    private const float DegenerateArea = 1e-9f;

    public static Plane[] CalculateFrustumPlanes(Camera camera) => CalculateFrustumPlanes(WorldToProjection(camera));

    public static Plane[] CalculateFrustumPlanes(Matrix4x4 worldToProjectionMatrix)
    {
        var planes = new Plane[PlaneCount];
        Extract(worldToProjectionMatrix, planes);
        return planes;
    }

    public static Plane[] CalculateFrustumPlanes(ref Matrix4x4 worldToProjectionMatrix) => CalculateFrustumPlanes(worldToProjectionMatrix);

    public static void CalculateFrustumPlanes(Camera camera, Span<Plane> planes) => Extract(WorldToProjection(camera), planes);

    public static void CalculateFrustumPlanes(Camera camera, Plane[] planes) => CalculateFrustumPlanes(WorldToProjection(camera), planes);

    public static void CalculateFrustumPlanes(Matrix4x4 worldToProjectionMatrix, Span<Plane> planes) => Extract(worldToProjectionMatrix, planes);

    public static void CalculateFrustumPlanes(ref Matrix4x4 worldToProjectionMatrix, Span<Plane> planes) => Extract(worldToProjectionMatrix, planes);

    public static void CalculateFrustumPlanes(Matrix4x4 worldToProjectionMatrix, Plane[] planes)
    {
        ArgumentNullException.ThrowIfNull(planes);
        Extract(worldToProjectionMatrix, planes);
    }

    public static void CalculateFrustumPlanes(ref Matrix4x4 worldToProjectionMatrix, Plane[] planes) => CalculateFrustumPlanes(worldToProjectionMatrix, planes);

    /// <summary>The axis-aligned box around the positions after the transform; an empty array gives an empty box at the origin.</summary>
    public static Bounds CalculateBounds(Vector3[] positions, Matrix4x4 transform)
    {
        ArgumentNullException.ThrowIfNull(positions);
        if (positions.Length == 0) return default;
        var first = transform.MultiplyPoint3x4(positions[0]);
        var bounds = new Bounds(first, Vector3.zero);
        for (var i = 1; i < positions.Length; i++) bounds.Encapsulate(transform.MultiplyPoint3x4(positions[i]));
        return bounds;
    }

    public static Bounds CalculateBounds(Vector3[] positions, ref Matrix4x4 transform) => CalculateBounds(positions, transform);

    /// <summary>
    /// Fits a plane to a polygon with Newell's method, so the normal follows the winding (counter-clockwise seen
    /// from the front, the same side <c>new Plane(a, b, c)</c> faces) and the plane passes through the
    /// vertex average. Returns false, with a default plane, for fewer than three vertices or a polygon whose
    /// area is zero.
    /// </summary>
    public static bool TryCreatePlaneFromPolygon(Vector3[] vertices, out Plane plane)
    {
        ArgumentNullException.ThrowIfNull(vertices);
        plane = default;
        if (vertices.Length < 3) return false;
        double nx = 0, ny = 0, nz = 0, cx = 0, cy = 0, cz = 0;
        for (var i = 0; i < vertices.Length; i++)
        {
            var (a, b) = (vertices[i], vertices[(i + 1) % vertices.Length]);
            nx += ((double)a.y - b.y) * ((double)a.z + b.z);
            ny += ((double)a.z - b.z) * ((double)a.x + b.x);
            nz += ((double)a.x - b.x) * ((double)a.y + b.y);
            (cx, cy, cz) = (cx + a.x, cy + a.y, cz + a.z);
        }
        var length = Math.Sqrt(nx * nx + ny * ny + nz * nz);
        if (length < DegenerateArea) return false;
        var count = vertices.Length;
        var normal = new Vector3((float)(nx / length), (float)(ny / length), (float)(nz / length));
        plane = new Plane(normal, new Vector3((float)(cx / count), (float)(cy / count), (float)(cz / count)));
        return true;
    }

    public static bool TestPlanesAABB(Plane[] planes, Bounds bounds)
    {
        ArgumentNullException.ThrowIfNull(planes);
        return TestPlanesAABB(planes, ref bounds);
    }

    public static bool TestPlanesAABB(Plane[] planes, ref Bounds bounds)
    {
        ArgumentNullException.ThrowIfNull(planes);
        return TestPlanesAABB((ReadOnlySpan<Plane>)planes, ref bounds);
    }

    /// <summary>True when the box is inside every plane or crosses it: the box corner furthest along each normal must not lie behind the plane.</summary>
    public static bool TestPlanesAABB(ReadOnlySpan<Plane> planes, ref Bounds bounds)
    {
        var (center, extents) = (bounds.center, bounds.extents);
        foreach (var plane in planes)
        {
            var n = plane.normal;
            var radius = extents.x * MathF.Abs(n.x) + extents.y * MathF.Abs(n.y) + extents.z * MathF.Abs(n.z);
            if (plane.GetDistanceToPoint(center) + radius < 0f) return false;
        }
        return true;
    }

    private static void Extract(Matrix4x4 m, Span<Plane> planes)
    {
        if (planes.Length < PlaneCount) throw new ArgumentException("The planes array needs room for six planes", nameof(planes));
        var (r0, r1, r2, r3) = (m.GetRow(0), m.GetRow(1), m.GetRow(2), m.GetRow(3));
        planes[0] = FromRow(r3 + r0);
        planes[1] = FromRow(r3 - r0);
        planes[2] = FromRow(r3 + r1);
        planes[3] = FromRow(r3 - r1);
        planes[4] = FromRow(r3 + r2);
        planes[5] = FromRow(r3 - r2);
    }

    private static Plane FromRow(Vector4 row)
    {
        var normal = new Vector3(row.x, row.y, row.z);
        var length = normal.magnitude;
        return length > 0f ? new Plane(normal / length, row.w / length) : default;
    }

    private static Matrix4x4 WorldToProjection(Camera camera)
    {
        ArgumentNullException.ThrowIfNull(camera);
        return camera.projectionMatrix * camera.worldToCameraMatrix;
    }
}
