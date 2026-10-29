using UnityEngine;

/// <summary>GeometryUtility planes, box tests, bounds and polygon planes against hand-computed values.</summary>
internal static class GeometryUtilityTests
{
    private const float Half = 0.70710678f;

    public static void Run()
    {
        Orthographic();
        Perspective();
        Overloads();
        BoxTests();
        BoundsOfPoints();
        Polygons();
        Console.WriteLine("PASS: GeometryUtility frustum planes, TestPlanesAABB, CalculateBounds and TryCreatePlaneFromPolygon");
    }

    private static void AssertPlane(Plane plane, Vector3 normal, float distance, string what)
    {
        Expect.Near(normal.x, plane.normal.x, what + " normal x");
        Expect.Near(normal.y, plane.normal.y, what + " normal y");
        Expect.Near(normal.z, plane.normal.z, what + " normal z");
        Expect.Near(distance, plane.distance, what + " distance", 1e-3f);
    }

    /// <summary>Ortho(-2, 2, -1, 1, 1, 10): x in [-2, 2], y in [-1, 1], z from -1 to -10 in front of the camera.</summary>
    private static void Orthographic()
    {
        var planes = GeometryUtility.CalculateFrustumPlanes(Matrix4x4.Ortho(-2f, 2f, -1f, 1f, 1f, 10f));
        Expect.Equal(6, planes.Length, "six planes");
        AssertPlane(planes[0], new Vector3(1, 0, 0), 2f, "left");
        AssertPlane(planes[1], new Vector3(-1, 0, 0), 2f, "right");
        AssertPlane(planes[2], new Vector3(0, 1, 0), 1f, "bottom");
        AssertPlane(planes[3], new Vector3(0, -1, 0), 1f, "top");
        AssertPlane(planes[4], new Vector3(0, 0, -1), -1f, "near");
        AssertPlane(planes[5], new Vector3(0, 0, 1), 10f, "far");
    }

    /// <summary>Perspective(90, 1, 1, 100): the side planes pass through the origin at 45 degrees.</summary>
    private static void Perspective()
    {
        var planes = GeometryUtility.CalculateFrustumPlanes(Matrix4x4.Perspective(90f, 1f, 1f, 100f));
        AssertPlane(planes[0], new Vector3(Half, 0, -Half), 0f, "left");
        AssertPlane(planes[1], new Vector3(-Half, 0, -Half), 0f, "right");
        AssertPlane(planes[2], new Vector3(0, Half, -Half), 0f, "bottom");
        AssertPlane(planes[3], new Vector3(0, -Half, -Half), 0f, "top");
        AssertPlane(planes[4], new Vector3(0, 0, -1), -1f, "near");
        AssertPlane(planes[5], new Vector3(0, 0, 1), 100f, "far");
        foreach (var plane in planes) Expect.Near(1f, plane.normal.magnitude, "planes are normalized");
        Expect.That(planes.All(p => p.GetDistanceToPoint(new Vector3(0, 0, -10)) > 0f), "normals point into the frustum");
    }

    private static void Overloads()
    {
        var matrix = Matrix4x4.Ortho(-2f, 2f, -1f, 1f, 1f, 10f);
        var expected = GeometryUtility.CalculateFrustumPlanes(matrix);
        var array = new Plane[6];
        GeometryUtility.CalculateFrustumPlanes(matrix, array);
        Expect.Sequence(expected.Select(p => p.ToString()), array.Select(p => p.ToString()), "array overload");
        var span = new Plane[7];
        GeometryUtility.CalculateFrustumPlanes(ref matrix, span.AsSpan());
        Expect.Sequence(expected.Select(p => p.ToString()), span.Take(6).Select(p => p.ToString()), "span overload with room to spare");
        Expect.Throws<ArgumentException>(() => GeometryUtility.CalculateFrustumPlanes(matrix, new Plane[5]), "five planes are too few");
        Expect.Throws<ArgumentNullException>(() => GeometryUtility.CalculateFrustumPlanes((Camera)null!), "a null camera");
    }

    private static void BoxTests()
    {
        var planes = GeometryUtility.CalculateFrustumPlanes(Matrix4x4.Perspective(90f, 1f, 1f, 100f));
        Expect.That(GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(0, 0, -10), Vector3.one)), "a box in front is inside");
        Expect.That(!GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(0, 0, 10), Vector3.one)), "a box behind the camera is outside");
        Expect.That(!GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(0, 0, -200), Vector3.one)), "a box past the far plane is outside");
        Expect.That(!GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(50, 0, -10), Vector3.one)), "a box far to the side is outside");
        Expect.That(GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(0, 0, -1), new Vector3(2, 2, 2))), "a box crossing the near plane intersects");
        Expect.That(GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(-10, 0, -10), new Vector3(4, 4, 4))), "a box on the left plane intersects");
        Expect.That(GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(-11, 0, -10), new Vector3(4, 4, 4))), "a box whose center is outside the left plane but whose corner reaches in intersects");
        Expect.That(!GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(-14, 0, -10), new Vector3(2, 2, 2))), "a box beyond the left plane is outside");
        var box = new Bounds(new Vector3(0, 0, -10), Vector3.one);
        Expect.That(GeometryUtility.TestPlanesAABB(planes, ref box) && GeometryUtility.TestPlanesAABB(planes.AsSpan(), ref box), "the ref and span overloads agree");
    }

    private static void BoundsOfPoints()
    {
        var matrix = Matrix4x4.TRS(new Vector3(10, 0, 0), Quaternion.identity, new Vector3(2, 2, 2));
        var bounds = GeometryUtility.CalculateBounds([new Vector3(1, 2, 3), new Vector3(-1, 0, 5)], matrix);
        Expect.That(bounds.center == new Vector3(10, 2, 8) && bounds.size == new Vector3(4, 4, 4), "the box around the transformed points");
        var single = GeometryUtility.CalculateBounds([new Vector3(1, 1, 1)], Matrix4x4.identity);
        Expect.That(single.center == new Vector3(1, 1, 1) && single.size == Vector3.zero, "one point gives an empty box there");
        Expect.That(GeometryUtility.CalculateBounds([], matrix) == default, "no points give the default box");
        Expect.Throws<ArgumentNullException>(() => GeometryUtility.CalculateBounds(null!, matrix), "null positions");
    }

    private static void Polygons()
    {
        var square = new[] { new Vector3(0, 0, 2), new Vector3(1, 0, 2), new Vector3(1, 1, 2), new Vector3(0, 1, 2) };
        Expect.That(GeometryUtility.TryCreatePlaneFromPolygon(square, out var plane), "a square makes a plane");
        AssertPlane(plane, new Vector3(0, 0, 1), -2f, "counter-clockwise square at z = 2");
        Array.Reverse(square);
        GeometryUtility.TryCreatePlaneFromPolygon(square, out var flipped);
        AssertPlane(flipped, new Vector3(0, 0, -1), 2f, "clockwise winding flips the normal");

        Expect.That(!GeometryUtility.TryCreatePlaneFromPolygon([new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(2, 0, 0)], out var line) && line == default, "collinear points are degenerate");
        Expect.That(!GeometryUtility.TryCreatePlaneFromPolygon([Vector3.one, Vector3.one, Vector3.one], out _), "coincident points are degenerate");
        Expect.That(!GeometryUtility.TryCreatePlaneFromPolygon([Vector3.zero, Vector3.one], out _), "two points are not a polygon");
        var tilted = new[] { new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 1), new Vector3(-1, 1, 1) };
        Expect.That(GeometryUtility.TryCreatePlaneFromPolygon(tilted, out var slanted), "a tilted quad");
        foreach (var vertex in tilted) Expect.Near(0f, slanted.GetDistanceToPoint(vertex), "every vertex lies on the plane");
    }
}
