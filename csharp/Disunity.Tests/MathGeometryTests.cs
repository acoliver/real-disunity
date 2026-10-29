using UnityEngine;

/// <summary>Known-value checks for Unity's rotation, matrix, plane, bounds, color and rectangle
/// types. Rotation expectations are computed here with plain trigonometry, not with the engine.</summary>
internal static class MathGeometryTests
{
    private const float Deg = MathF.PI / 180f;

    public static void Run()
    {
        QuaternionEulerOrder();
        QuaternionEulerRoundTrip();
        QuaternionOperations();
        MatrixTransforms();
        MatrixInverseAndAccess();
        MatrixProjections();
        MatrixPlanesAndValidity();
        PlaneAndRay();
        BoundsBehavior();
        ColorBehavior();
        Color32Behavior();
        RectBehavior();
        IntegerRectsAndBoxes();
        Console.WriteLine("PASS: Unity math geometry values (Quaternion, Matrix4x4, Plane, Ray, Bounds, Color, Color32, Rect, RectInt, BoundsInt)");
    }

    private static void Require(bool condition, string message) => MathTests.Require(condition, message);
    private static void RequireNear(float actual, float expected, string message, float tolerance = 1e-4f) => MathTests.RequireNear(actual, expected, message, tolerance);
    private static void RequireNear(Vector3 actual, Vector3 expected, string message, float tolerance = 1e-4f) => MathTests.RequireNear(actual, expected, message, tolerance);

    private static Vector3 RotateX(Vector3 v, float degrees)
    {
        float c = MathF.Cos(degrees * Deg), s = MathF.Sin(degrees * Deg);
        return new Vector3(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
    }

    private static Vector3 RotateY(Vector3 v, float degrees)
    {
        float c = MathF.Cos(degrees * Deg), s = MathF.Sin(degrees * Deg);
        return new Vector3(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
    }

    private static Vector3 RotateZ(Vector3 v, float degrees)
    {
        float c = MathF.Cos(degrees * Deg), s = MathF.Sin(degrees * Deg);
        return new Vector3(v.x * c - v.y * s, v.x * s + v.y * c, v.z);
    }

    private static Vector3 ZxyRotation(Vector3 v, float x, float y, float z) => RotateY(RotateX(RotateZ(v, z), x), y);

    private static void QuaternionEulerOrder()
    {
        var q = Quaternion.Euler(30f, 60f, 90f);
        var probe = new Vector3(1f, 2f, 3f);
        RequireNear(q * probe, ZxyRotation(probe, 30f, 60f, 90f), "Euler applies Z, then X, then Y");
        RequireNear(q * Vector3.forward, ZxyRotation(Vector3.forward, 30f, 60f, 90f), "Euler(30, 60, 90) applied to forward");
        RequireNear(q * Vector3.forward, new Vector3(0.75f, -0.5f, 0.4330127f), "forward after Euler(30, 60, 90)");
        Require((q * probe - RotateX(RotateY(RotateZ(probe, 90f), 60f), 30f)).magnitude > 0.1f, "the Euler order is not X, Y, Z");
        RequireNear(Quaternion.Euler(0f, 90f, 0f) * Vector3.forward, Vector3.right, "a quarter turn about Y turns forward to right");
        RequireNear(Quaternion.Euler(90f, 0f, 0f) * Vector3.forward, Vector3.down, "a quarter turn about X turns forward down");
        RequireNear(Quaternion.Euler(0f, 0f, 90f) * Vector3.right, Vector3.up, "a quarter turn about Z turns right up");
        RequireNear(Quaternion.Euler(new Vector3(30f, 60f, 90f)) * probe, q * probe, "Euler(Vector3) matches Euler(x, y, z)");
        var fromProperty = Quaternion.identity;
        fromProperty.eulerAngles = new Vector3(30f, 60f, 90f);
        RequireNear(fromProperty * probe, q * probe, "setting eulerAngles matches Euler");
        RequireNear(Matrix4x4.Rotate(q).MultiplyVector(probe), q * probe, "the rotation matrix agrees with the quaternion");
        RequireNear(Quaternion.AngleAxis(90f, Vector3.up) * Vector3.forward, Vector3.right, "AngleAxis about up");
        Require(Quaternion.Angle(Quaternion.AngleAxis(90f, Vector3.up), Quaternion.Euler(0f, 90f, 0f)) < 0.01f, "AngleAxis matches Euler");
        Require(Quaternion.AngleAxis(45f, Vector3.zero).Equals(Quaternion.identity), "AngleAxis about a zero axis is the identity");
        RequireNear(Quaternion.AngleAxis(90f, new Vector3(0f, 5f, 0f)) * Vector3.forward, Vector3.right, "AngleAxis normalizes its axis");
    }

    private static void QuaternionEulerRoundTrip()
    {
        RequireNear(Quaternion.Euler(10f, 20f, 30f).eulerAngles, new Vector3(10f, 20f, 30f), "eulerAngles of Euler(10, 20, 30)", 1e-3f);
        RequireNear(Quaternion.Euler(30f, 60f, 90f).eulerAngles, new Vector3(30f, 60f, 90f), "eulerAngles of Euler(30, 60, 90)", 1e-3f);
        RequireNear(Quaternion.Euler(0f, 90f, 0f).eulerAngles, new Vector3(0f, 90f, 0f), "eulerAngles of a turn about Y", 1e-3f);
        RequireNear(Quaternion.Euler(0f, 270f, 0f).eulerAngles, new Vector3(0f, 270f, 0f), "eulerAngles stay in [0, 360)", 1e-3f);
        RequireNear(Quaternion.Euler(-10f, 0f, 0f).eulerAngles, new Vector3(350f, 0f, 0f), "negative angles wrap to 360", 1e-3f);
        RequireNear(Quaternion.Euler(90f, 0f, 0f).eulerAngles, new Vector3(90f, 0f, 0f), "eulerAngles at the X pole", 1e-3f);
        RequireNear(Quaternion.identity.eulerAngles, Vector3.zero, "eulerAngles of the identity");
        var back = Quaternion.Euler(123f, 45f, 300f).eulerAngles;
        Require(Quaternion.Angle(Quaternion.Euler(back), Quaternion.Euler(123f, 45f, 300f)) < 0.01f, "eulerAngles describe the same rotation");
    }

    private static void QuaternionOperations()
    {
        var turn = Quaternion.Euler(0f, 90f, 0f);
        RequireNear(Quaternion.Angle(Quaternion.identity, turn), 90f, "Angle between rotations", 0.01f);
        var q = Quaternion.Euler(10f, 20f, 30f);
        var negated = new Quaternion(-q.x, -q.y, -q.z, -q.w);
        Require(Quaternion.Angle(q, negated) == 0f && Quaternion.Angle(q, q) == 0f, "Angle is 0 for equal rotations, including the negated one");
        Require(Quaternion.Angle(Quaternion.identity, Quaternion.Slerp(Quaternion.identity, turn, 0.5f)) is > 44.99f and < 45.01f, "Slerp halfway turns 45 degrees");
        Require(Quaternion.Angle(Quaternion.Slerp(Quaternion.identity, turn, 0.5f), Quaternion.Euler(0f, 45f, 0f)) < 0.01f, "Slerp halfway is Euler(0, 45, 0)");
        Require(Quaternion.Angle(Quaternion.Slerp(Quaternion.identity, turn, 2f), turn) < 0.01f, "Slerp clamps t");
        Require(Quaternion.Angle(Quaternion.SlerpUnclamped(Quaternion.identity, turn, 2f), Quaternion.Euler(0f, 180f, 0f)) < 0.01f, "SlerpUnclamped extrapolates");
        var lerped = Quaternion.Lerp(Quaternion.identity, turn, 0.5f);
        RequireNear(MathF.Sqrt(Quaternion.Dot(lerped, lerped)), 1f, "Lerp normalizes", 1e-5f);
        Require(Quaternion.Angle(lerped, Quaternion.Euler(0f, 45f, 0f)) < 0.01f, "Lerp halfway");
        Require(Quaternion.Angle(Quaternion.Lerp(Quaternion.identity, new Quaternion(0f, -0.70710678f, 0f, -0.70710678f), 0.5f), Quaternion.Euler(0f, 45f, 0f)) < 0.01f, "Lerp takes the shortest arc");
        Require(Quaternion.Angle(q * Quaternion.Inverse(q), Quaternion.identity) < 0.01f, "a rotation times its inverse is the identity");
        RequireNear(Quaternion.FromToRotation(Vector3.right, Vector3.up) * Vector3.right, Vector3.up, "FromToRotation");
        RequireNear(Quaternion.FromToRotation(Vector3.right, Vector3.left) * Vector3.right, Vector3.left, "FromToRotation of opposite directions");
        Require(Quaternion.FromToRotation(Vector3.up, Vector3.up).Equals(Quaternion.identity), "FromToRotation of equal directions");
        RequireNear(Quaternion.Angle(Quaternion.identity, Quaternion.RotateTowards(Quaternion.identity, turn, 30f)), 30f, "RotateTowards by 30 degrees", 0.01f);
        Require(Quaternion.RotateTowards(Quaternion.identity, turn, 120f) == turn, "RotateTowards stops at the target");
        Require(Quaternion.Normalize(new Quaternion(0f, 0f, 0f, 0f)).Equals(Quaternion.identity), "normalizing a zero quaternion gives the identity");
        RequireNear(Quaternion.Normalize(new Quaternion(0f, 0f, 0f, 4f)).w, 1f, "Normalize scales to unit length");
        Require(Quaternion.Angle(turn * turn, Quaternion.Euler(0f, 180f, 0f)) < 0.01f, "quaternion product composes rotations");
        RequireNear((turn * Quaternion.Euler(90f, 0f, 0f)) * Vector3.forward, turn * (Quaternion.Euler(90f, 0f, 0f) * Vector3.forward), "product applies the right operand first");
        LookRotations();
        var axis = Vector3.zero;
        var angle = 0f;
        turn.ToAngleAxis(ref angle, ref axis);
        RequireNear(angle, 90f, "ToAngleAxis angle", 0.01f);
        RequireNear(axis, Vector3.up, "ToAngleAxis axis");
        Quaternion.identity.ToAngleAxis(ref angle, ref axis);
        RequireNear(angle, 0f, "ToAngleAxis of the identity", 0.01f);
        var indexed = new Quaternion(1f, 2f, 3f, 4f);
        indexed[3] = 9f;
        Require(indexed.w == 9f && indexed[0] == 1f, "Quaternion indexer");
    }

    private static void LookRotations()
    {
        RequireNear(Quaternion.LookRotation(Vector3.right) * Vector3.forward, Vector3.right, "LookRotation points forward at the target");
        RequireNear(Quaternion.LookRotation(Vector3.right).eulerAngles, new Vector3(0f, 90f, 0f), "LookRotation(right) turns 90 degrees about Y", 1e-3f);
        Require(Quaternion.Angle(Quaternion.LookRotation(Vector3.forward), Quaternion.identity) < 0.01f, "LookRotation(forward) is the identity");
        Require(Quaternion.LookRotation(Vector3.zero).Equals(Quaternion.identity), "LookRotation of a zero vector is the identity");
        var look = Quaternion.LookRotation(new Vector3(1f, 1f, 0f), Vector3.up);
        RequireNear(look * Vector3.forward, new Vector3(0.70710678f, 0.70710678f, 0f), "LookRotation diagonal forward");
        RequireNear(look * Vector3.right, new Vector3(0f, 0f, -1f), "LookRotation keeps right horizontal");
        RequireNear(Quaternion.LookRotation(Vector3.forward, Vector3.right) * Vector3.up, Vector3.right, "LookRotation honours the up hint");
        RequireNear(Quaternion.LookRotation(Vector3.up) * Vector3.forward, Vector3.up, "LookRotation straight along up");
        var set = Quaternion.identity;
        set.SetLookRotation(Vector3.right);
        RequireNear(set * Vector3.forward, Vector3.right, "SetLookRotation");
        set.SetFromToRotation(Vector3.right, Vector3.up);
        RequireNear(set * Vector3.right, Vector3.up, "SetFromToRotation");
    }

    private static Matrix4x4 SampleTrs() => Matrix4x4.TRS(new Vector3(1f, 2f, 3f), Quaternion.Euler(0f, 90f, 0f), new Vector3(2f, 3f, 4f));

    private static void MatrixTransforms()
    {
        var m = SampleTrs();
        var byHand = RotateY(new Vector3(2f, 3f, 4f), 90f) + new Vector3(1f, 2f, 3f);
        RequireNear(byHand, new Vector3(5f, 5f, 1f), "hand computed TRS result");
        RequireNear(m.MultiplyPoint3x4(Vector3.one), byHand, "TRS times a point scales, rotates, then translates");
        RequireNear(m.MultiplyPoint(Vector3.one), byHand, "MultiplyPoint agrees for an affine matrix");
        RequireNear(m.MultiplyVector(Vector3.one), byHand - new Vector3(1f, 2f, 3f), "MultiplyVector ignores translation");
        var product = m * new Vector4(1f, 1f, 1f, 1f);
        Require(Near4(product, new Vector4(5f, 5f, 1f, 1f)), "matrix times Vector4");
        RequireNear(m.GetPosition(), new Vector3(1f, 2f, 3f), "GetPosition");
        RequireNear(m.lossyScale, new Vector3(2f, 3f, 4f), "lossyScale");
        Require(Quaternion.Angle(m.rotation, Quaternion.Euler(0f, 90f, 0f)) < 0.01f, "rotation");
        var translateThenScale = Matrix4x4.Translate(new Vector3(1f, 0f, 0f)) * Matrix4x4.Scale(new Vector3(2f, 2f, 2f));
        RequireNear(translateThenScale.MultiplyPoint3x4(Vector3.one), new Vector3(3f, 2f, 2f), "the right matrix applies first");
        var scaleThenTranslate = Matrix4x4.Scale(new Vector3(2f, 2f, 2f)) * Matrix4x4.Translate(new Vector3(1f, 0f, 0f));
        RequireNear(scaleThenTranslate.MultiplyPoint3x4(Vector3.one), new Vector3(4f, 2f, 2f), "matrix products do not commute");
        var set = Matrix4x4.identity;
        set.SetTRS(new Vector3(1f, 2f, 3f), Quaternion.Euler(0f, 90f, 0f), new Vector3(2f, 3f, 4f));
        Require(set == m, "SetTRS matches TRS");
        Require(Matrix4x4.identity.isIdentity && !m.isIdentity && Matrix4x4.Translate(Vector3.zero).isIdentity, "isIdentity");
        Require(Near4(m.GetColumn(3), new Vector4(1f, 2f, 3f, 1f)) && Near4(m.GetRow(3), new Vector4(0f, 0f, 0f, 1f)), "GetColumn and GetRow");
        var edited = Matrix4x4.identity;
        edited.SetColumn(3, new Vector4(7f, 8f, 9f, 1f));
        edited.SetRow(0, new Vector4(2f, 0f, 0f, 7f));
        Require(edited.m03 == 7f && edited.m13 == 8f && edited.m00 == 2f, "SetColumn and SetRow");
    }

    private static bool Near4(Vector4 a, Vector4 b) =>
        MathTests.Near(a.x, b.x) && MathTests.Near(a.y, b.y) && MathTests.Near(a.z, b.z) && MathTests.Near(a.w, b.w);

    private static bool NearMatrix(Matrix4x4 a, Matrix4x4 b, float tolerance = 1e-4f)
    {
        for (var i = 0; i < 16; i++)
        {
            if (!MathTests.Near(a[i], b[i], tolerance)) return false;
        }
        return true;
    }

    private static void MatrixInverseAndAccess()
    {
        var m = SampleTrs();
        Require(NearMatrix(m.inverse * m, Matrix4x4.identity, 1e-5f), "inverse times matrix is the identity");
        Require(NearMatrix(m * m.inverse, Matrix4x4.identity, 1e-5f), "matrix times inverse is the identity");
        var general = new Matrix4x4(new Vector4(2f, 1f, 0f, 1f), new Vector4(0f, 3f, 1f, 0f), new Vector4(1f, 0f, 4f, 2f), new Vector4(0f, 1f, 1f, 5f));
        Require(NearMatrix(Matrix4x4.Inverse(general) * general, Matrix4x4.identity, 1e-5f), "inverse of a general matrix");
        RequireNear(SampleTrs().determinant, 24f, "determinant of a TRS matrix is the product of the scales", 1e-3f);
        RequireNear(Matrix4x4.Determinant(Matrix4x4.identity), 1f, "determinant of the identity");
        RequireNear(Matrix4x4.Scale(new Vector3(2f, 3f, -4f)).determinant, -24f, "determinant of a mirroring scale");
        RequireNear(general.determinant, 119f, "determinant of a general matrix", 1e-2f);
        Require(Matrix4x4.zero.inverse == Matrix4x4.zero, "a singular matrix inverts to zero");
        var inverseAffine = Matrix4x4.identity;
        Require(Matrix4x4.Inverse3DAffine(m, ref inverseAffine) && NearMatrix(inverseAffine, m.inverse, 1e-5f), "Inverse3DAffine matches Inverse");
        var untouched = Matrix4x4.identity;
        Require(!Matrix4x4.Inverse3DAffine(Matrix4x4.zero, ref untouched), "Inverse3DAffine refuses a singular matrix");
        Require(m.transpose.transpose == m && m.transpose.m10 == m.m01 && Matrix4x4.Transpose(m).m03 == m.m30, "transpose swaps rows and columns");
        var probe = Matrix4x4.identity;
        probe.m01 = 5f;
        Require(probe[0, 1] == 5f && probe[4] == 5f && probe[1, 0] == 0f, "the indexers address column-major storage");
        probe[2, 3] = 7f;
        Require(probe.m23 == 7f && probe[14] == 7f, "indexer assignment");
        probe[15] = 3f;
        Require(probe.m33 == 3f, "flat indexer assignment");
        try
        {
            _ = probe[16];
            Require(false, "matrix indexer must reject index 16");
        }
        catch (IndexOutOfRangeException)
        {
        }
        Require(m == SampleTrs() && m.Equals(SampleTrs()) && !(m == Matrix4x4.identity) && m != Matrix4x4.identity, "matrix equality");
        Require(Matrix4x4.zero.m00 == 0f && Matrix4x4.identity.m22 == 1f, "zero and identity");
    }

    private static void MatrixProjections()
    {
        var ortho = Matrix4x4.Ortho(-2f, 2f, -1f, 1f, 0.1f, 100f);
        RequireNear(ortho.MultiplyPoint(new Vector3(-2f, -1f, -0.1f)), new Vector3(-1f, -1f, -1f), "Ortho maps the near corner", 1e-3f);
        RequireNear(ortho.MultiplyPoint(new Vector3(2f, 1f, -100f)), Vector3.one, "Ortho maps the far corner", 1e-3f);
        RequireNear(ortho.MultiplyPoint(new Vector3(0f, 0f, -50.05f)), Vector3.zero, "Ortho maps the center", 1e-3f);
        var perspective = Matrix4x4.Perspective(90f, 2f, 1f, 100f);
        RequireNear(perspective.MultiplyPoint(new Vector3(2f, 1f, -1f)), new Vector3(1f, 1f, -1f), "Perspective maps the near corner", 1e-4f);
        RequireNear(perspective.MultiplyPoint(new Vector3(0f, 0f, -100f)), new Vector3(0f, 0f, 1f), "Perspective maps the far plane center", 1e-4f);
        RequireNear(perspective.MultiplyPoint(new Vector3(100f, 50f, -50f)), new Vector3(1f, 1f, 0.979798f), "Perspective divides by depth", 1e-3f);
        Require(NearMatrix(Matrix4x4.Frustum(-2f, 2f, -1f, 1f, 1f, 100f), perspective, 1e-5f), "Frustum matches Perspective for a symmetric view");
        var planes = perspective.decomposeProjection;
        Require(MathTests.Near(planes.left, -2f) && MathTests.Near(planes.right, 2f) && MathTests.Near(planes.bottom, -1f)
            && MathTests.Near(planes.top, 1f) && MathTests.Near(planes.zNear, 1f) && MathTests.Near(planes.zFar, 100f, 1e-3f), "decomposeProjection of a perspective matrix");
        var orthoPlanes = ortho.decomposeProjection;
        Require(MathTests.Near(orthoPlanes.left, -2f) && MathTests.Near(orthoPlanes.right, 2f) && MathTests.Near(orthoPlanes.zNear, 0.1f, 1e-3f)
            && MathTests.Near(orthoPlanes.zFar, 100f, 1e-2f), "decomposeProjection of an orthographic matrix");
        Require(NearMatrix(Matrix4x4.Frustum(planes), perspective, 1e-4f), "Frustum from FrustumPlanes");
        // LookAt is TRS(from, LookRotation(to - from, up), one): the viewer's transform, which
        // maps its local forward onto the line toward the target.
        var look = Matrix4x4.LookAt(new Vector3(1f, 2f, 3f), new Vector3(1f, 2f, 13f), Vector3.up);
        RequireNear(look.MultiplyPoint3x4(Vector3.zero), new Vector3(1f, 2f, 3f), "LookAt places the viewer at from");
        RequireNear(look.MultiplyPoint3x4(new Vector3(0f, 0f, 10f)), new Vector3(1f, 2f, 13f), "LookAt maps local forward to the target");
        var sideways = Matrix4x4.LookAt(Vector3.zero, Vector3.right, Vector3.up);
        RequireNear(sideways.MultiplyPoint3x4(new Vector3(0f, 0f, 5f)), new Vector3(5f, 0f, 0f), "LookAt toward right");
    }

    private static void MatrixPlanesAndValidity()
    {
        var floor = new Plane(Vector3.up, new Vector3(0f, 1f, 0f));
        var raised = Matrix4x4.Translate(new Vector3(0f, 5f, 0f)).TransformPlane(floor);
        RequireNear(raised.normal, Vector3.up, "TransformPlane keeps the normal under translation");
        RequireNear(raised.distance, -6f, "TransformPlane moves the plane");
        var doubled = Matrix4x4.Scale(new Vector3(2f, 2f, 2f)).TransformPlane(floor);
        RequireNear(doubled.normal, Vector3.up, "TransformPlane renormalizes");
        RequireNear(doubled.distance, -2f, "TransformPlane under scale");
        Require(SampleTrs().ValidTRS(), "a TRS matrix is a valid TRS");
        Require(!Matrix4x4.Scale(new Vector3(1f, 0f, 1f)).ValidTRS(), "a zero scale is not a valid TRS");
        Require(!Matrix4x4.Perspective(60f, 1f, 0.1f, 10f).ValidTRS(), "a projection is not a valid TRS");
        var sheared = Matrix4x4.identity;
        sheared.m01 = 1f;
        Require(!sheared.ValidTRS(), "a shear is not a valid TRS");
    }

    private static void PlaneAndRay()
    {
        var plane = new Plane(Vector3.up, new Vector3(0f, 2f, 0f));
        RequireNear(plane.distance, -2f, "Plane distance is minus the offset along the normal");
        RequireNear(plane.GetDistanceToPoint(new Vector3(0f, 5f, 0f)), 3f, "signed distance above");
        RequireNear(plane.GetDistanceToPoint(new Vector3(0f, -1f, 0f)), -3f, "signed distance below");
        Require(plane.GetSide(new Vector3(0f, 3f, 0f)) && !plane.GetSide(new Vector3(0f, 1f, 0f)), "GetSide");
        Require(plane.SameSide(new Vector3(0f, 3f, 0f), new Vector3(9f, 4f, 0f)) && !plane.SameSide(new Vector3(0f, 3f, 0f), Vector3.zero), "SameSide");
        RequireNear(plane.ClosestPointOnPlane(new Vector3(3f, 5f, 1f)), new Vector3(3f, 2f, 1f), "ClosestPointOnPlane");
        var enter = 0f;
        Require(plane.Raycast(new Ray(new Vector3(0f, 5f, 0f), Vector3.down), ref enter), "a ray toward the plane hits");
        RequireNear(enter, 3f, "Raycast distance");
        Require(!plane.Raycast(new Ray(new Vector3(0f, 5f, 0f), Vector3.up), ref enter) && enter < 0f, "a ray pointing away misses with a negative distance");
        Require(!plane.Raycast(new Ray(new Vector3(0f, 5f, 0f), Vector3.right), ref enter) && enter == 0f, "a parallel ray misses with distance 0");
        var slanted = new Ray(new Vector3(0f, 6f, 0f), new Vector3(1f, -2f, 0f));
        Require(plane.Raycast(slanted, ref enter), "a slanted ray hits");
        RequireNear(slanted.GetPoint(enter).y, 2f, "the Raycast distance reaches the plane");
        var flipped = plane.flipped;
        RequireNear(flipped.normal, Vector3.down, "flipped normal");
        RequireNear(flipped.distance, 2f, "flipped distance");
        var moved = plane;
        // As in Unity, the distance grows by the translation along the normal, so the plane
        // at y = 2 moves to y = -1.
        moved.Translate(new Vector3(0f, 3f, 0f));
        RequireNear(moved.GetDistanceToPoint(new Vector3(0f, -1f, 0f)), 0f, "Translate adds to the distance as Unity does");
        RequireNear(Plane.Translate(plane, new Vector3(9f, 3f, 9f)).GetDistanceToPoint(new Vector3(0f, -1f, 0f)), 0f, "static Translate ignores sliding");
        var spanned = new Plane(Vector3.zero, new Vector3(0f, 0f, 1f), new Vector3(1f, 0f, 0f));
        RequireNear(spanned.normal, Vector3.up, "a plane from three clockwise points faces up");
        spanned.Flip();
        RequireNear(spanned.normal, Vector3.down, "Flip");
        var set = new Plane();
        set.SetNormalAndPosition(new Vector3(0f, 0f, 2f), new Vector3(0f, 0f, 4f));
        RequireNear(set.distance, -4f, "SetNormalAndPosition");
        Require(plane == new Plane(Vector3.up, new Vector3(5f, 2f, 5f)) && plane != flipped, "plane equality");
        var ray = new Ray(new Vector3(1f, 1f, 1f), new Vector3(0f, 0f, 5f));
        Require(ray.direction.Equals(Vector3.forward) && ray.origin.Equals(Vector3.one), "Ray normalizes its direction");
        RequireNear(ray.GetPoint(3f), new Vector3(1f, 1f, 4f), "Ray.GetPoint");
        ray.direction = new Vector3(0f, 4f, 0f);
        Require(ray.direction.Equals(Vector3.up), "setting the direction normalizes it");
    }

    private static void BoundsBehavior()
    {
        var box = new Bounds(Vector3.one, new Vector3(2f, 2f, 2f));
        RequireNear(box.min, Vector3.zero, "Bounds.min");
        RequireNear(box.max, new Vector3(2f, 2f, 2f), "Bounds.max");
        RequireNear(box.extents, Vector3.one, "Bounds.extents");
        box.Encapsulate(new Vector3(5f, 1f, 1f));
        RequireNear(box.min, Vector3.zero, "Encapsulate keeps the minimum");
        RequireNear(box.max, new Vector3(5f, 2f, 2f), "Encapsulate grows the maximum");
        RequireNear(box.center, new Vector3(2.5f, 1f, 1f), "Encapsulate recenters");
        RequireNear(box.size, new Vector3(5f, 2f, 2f), "Encapsulate resizes");
        box.Encapsulate(new Bounds(new Vector3(-3f, 0f, 0f), new Vector3(2f, 2f, 2f)));
        RequireNear(box.min, new Vector3(-4f, -1f, -1f), "Encapsulate(Bounds) grows the minimum");
        RequireNear(box.max, new Vector3(5f, 2f, 2f), "Encapsulate(Bounds) keeps the maximum");
        var grown = new Bounds(Vector3.zero, Vector3.one);
        grown.Expand(2f);
        RequireNear(grown.size, new Vector3(3f, 3f, 3f), "Expand(float) adds to the size");
        grown.Expand(new Vector3(2f, 4f, 6f));
        RequireNear(grown.size, new Vector3(5f, 7f, 9f), "Expand(Vector3) adds to the size");
        var unit = new Bounds(Vector3.one, new Vector3(2f, 2f, 2f));
        Require(unit.Contains(new Vector3(2f, 2f, 2f)) && unit.Contains(Vector3.one) && !unit.Contains(new Vector3(2.001f, 1f, 1f)), "Contains includes the surface");
        Require(unit.Intersects(new Bounds(new Vector3(3f, 1f, 1f), new Vector3(2f, 2f, 2f))), "touching boxes intersect");
        Require(!unit.Intersects(new Bounds(new Vector3(4f, 1f, 1f), new Vector3(2f, 2f, 2f))), "separate boxes do not intersect");
        var distance = 0f;
        Require(unit.IntersectRay(new Ray(new Vector3(-5f, 1f, 1f), Vector3.right), ref distance), "a ray toward the box hits");
        RequireNear(distance, 5f, "IntersectRay entry distance");
        Require(unit.IntersectRay(new Ray(Vector3.one, Vector3.right), ref distance) && distance == 0f, "a ray from inside hits at distance 0");
        Require(!unit.IntersectRay(new Ray(new Vector3(-5f, 1f, 1f), Vector3.left)), "a ray pointing away misses");
        Require(!unit.IntersectRay(new Ray(new Vector3(-5f, 10f, 1f), Vector3.right)), "a ray passing above misses");
        Require(unit.IntersectRay(new Ray(new Vector3(1f, -5f, 1f), Vector3.up), ref distance) && MathTests.Near(distance, 5f), "a ray along an axis hits");
        Require(unit.IntersectRay(new Ray(new Vector3(-1f, -1f, 1f), new Vector3(1f, 1f, 0f))), "a diagonal ray hits");
        RequireNear(unit.SqrDistance(new Vector3(5f, 1f, 1f)), 9f, "SqrDistance");
        RequireNear(unit.SqrDistance(Vector3.one), 0f, "SqrDistance inside");
        RequireNear(unit.ClosestPoint(new Vector3(5f, 5f, 1f)), new Vector3(2f, 2f, 1f), "ClosestPoint");
        var minMax = new Bounds();
        minMax.SetMinMax(Vector3.zero, new Vector3(4f, 2f, 6f));
        RequireNear(minMax.center, new Vector3(2f, 1f, 3f), "SetMinMax center");
        RequireNear(minMax.size, new Vector3(4f, 2f, 6f), "SetMinMax size");
        minMax.min = new Vector3(-2f, 0f, 0f);
        RequireNear(minMax.max, new Vector3(4f, 2f, 6f), "setting min keeps max");
        minMax.size = new Vector3(2f, 2f, 2f);
        RequireNear(minMax.center, new Vector3(1f, 1f, 3f), "setting size keeps the center");
        Require(unit == new Bounds(new Vector3(1.000001f, 1f, 1f), new Vector3(2f, 2f, 2f)) && !unit.Equals(new Bounds(new Vector3(1.000001f, 1f, 1f), new Vector3(2f, 2f, 2f))), "Bounds == is approximate and Equals exact");
    }

    private static void ColorBehavior()
    {
        float h = 0, s = 0, v = 0;
        Color.RGBToHSV(Color.red, ref h, ref s, ref v);
        Require(h == 0f && s == 1f && v == 1f, "RGBToHSV of red");
        Color.RGBToHSV(Color.green, ref h, ref s, ref v);
        RequireNear(h, 1f / 3f, "hue of green", 1e-6f);
        Color.RGBToHSV(Color.blue, ref h, ref s, ref v);
        RequireNear(h, 2f / 3f, "hue of blue", 1e-6f);
        Color.RGBToHSV(new Color(1f, 1f, 0f), ref h, ref s, ref v);
        RequireNear(h, 1f / 6f, "hue of yellow", 1e-6f);
        Color.RGBToHSV(new Color(1f, 0f, 1f), ref h, ref s, ref v);
        RequireNear(h, 5f / 6f, "hue of magenta", 1e-6f);
        Color.RGBToHSV(new Color(0.5f, 0.5f, 0.5f), ref h, ref s, ref v);
        Require(s == 0f && v == 0.5f, "gray has no saturation");
        foreach (var color in new[] { new Color(0.2f, 0.4f, 0.6f), new Color(0.9f, 0.1f, 0.3f), new Color(0.5f, 0.5f, 0.5f), new Color(0.1f, 0.8f, 0.2f), Color.white, Color.black })
        {
            Color.RGBToHSV(color, ref h, ref s, ref v);
            var back = Color.HSVToRGB(h, s, v);
            Require(Math.Abs(back.r - color.r) < 1e-5f && Math.Abs(back.g - color.g) < 1e-5f && Math.Abs(back.b - color.b) < 1e-5f, $"HSV round trip of {color}");
        }
        Require(Color.HSVToRGB(0.5f, 1f, 1f) == Color.cyan, "HSVToRGB(0.5, 1, 1) is cyan");
        Require(Color.HSVToRGB(0f, 0f, 2f).Equals(new Color(2f, 2f, 2f, 1f)) && Color.HSVToRGB(0f, 0f, 2f, false).Equals(Color.white), "HSVToRGB keeps or clamps values above one");
        Require(Color.Lerp(Color.black, Color.white, 2f) == Color.white && Color.LerpUnclamped(Color.black, Color.white, 2f) == new Color(2f, 2f, 2f, 1f), "Lerp clamps and LerpUnclamped does not");
        Require(Color.red + Color.green == new Color(1f, 1f, 0f, 2f) && Color.white - Color.red == new Color(0f, 1f, 1f, 0f), "Color + and -");
        Require(Color.white * 0.5f == new Color(0.5f, 0.5f, 0.5f, 0.5f) && 2f * Color.gray == new Color(1f, 1f, 1f, 2f) && Color.white / 2f == new Color(0.5f, 0.5f, 0.5f, 0.5f), "Color scaling");
        Require(Color.red * Color.green == new Color(0f, 0f, 0f, 1f), "Color * Color is componentwise");
        RequireNear(Color.white.grayscale, 1f, "grayscale of white", 1e-6f);
        RequireNear(Color.green.grayscale, 0.587f, "grayscale of green", 1e-6f);
        RequireNear(new Color(0.2f, 0.9f, 0.5f).maxColorComponent, 0.9f, "maxColorComponent");
        var linear = new Color(0.5f, 0.5f, 0.5f, 0.3f).linear;
        RequireNear(linear.r, 0.214041f, "Color.linear", 1e-5f);
        RequireNear(linear.a, 0.3f, "Color.linear leaves alpha alone");
        RequireNear(linear.gamma.r, 0.5f, "Color.gamma undoes Color.linear", 1e-5f);
        Require(Color.yellow.r == 1f && Color.yellow.g == 0.92156863f && Color.yellow.b == 0.015686275f && Color.yellow.a == 1f, "Color.yellow");
        Require(Color.clear.Equals(new Color(0f, 0f, 0f, 0f)) && Color.gray.Equals(new Color(0.5f, 0.5f, 0.5f, 1f)) && Color.grey.Equals(Color.gray), "named colors");
        Require(Color.cornflowerBlue.Equals(new Color(0.3921569f, 0.5843138f, 0.9294118f, 1f)) && Color.gray1.r == 0.1f && Color.gray9.g == 0.9f, "Unity 6 color values");
        var indexed = new Color(0.1f, 0.2f, 0.3f, 0.4f);
        indexed[2] = 0.9f;
        Require(indexed.b == 0.9f && indexed[3] == 0.4f, "Color indexer");
    }

    private static void Color32Behavior()
    {
        Color32 rounded = new Color(0.5f, 0.2f, 1.2f, -1f);
        Require(rounded.r == 128 && rounded.g == 51 && rounded.b == 255 && rounded.a == 0, "Color to Color32 rounds half to even and clamps");
        Color32 nearHalf = new Color(0.498f, 0.502f, 0.999f, 1f);
        Require(nearHalf.r == 127 && nearHalf.g == 128 && nearHalf.b == 255 && nearHalf.a == 255, "Color to Color32 rounds to the nearest byte");
        Color widened = new Color32(255, 0, 51, 255);
        Require(widened.Equals(new Color(1f, 0f, 0.2f, 1f)), "Color32 to Color divides by 255");
        var half = Color32.Lerp(new Color32(0, 0, 0, 255), new Color32(255, 255, 255, 255), 0.5f);
        Require(half.r == 127 && half.a == 255, "Color32.Lerp");
        Require(Color32.Lerp(new Color32(0, 0, 0, 255), new Color32(255, 255, 255, 255), 5f).g == 255, "Color32.Lerp clamps t");
        Require(Color32.LerpUnclamped(new Color32(100, 0, 0, 255), new Color32(200, 0, 0, 255), 10f).r == 255, "Color32.LerpUnclamped stays within a byte");
        var indexed = new Color32(1, 2, 3, 4);
        indexed[1] = 9;
        Require(indexed[1] == 9 && indexed[3] == 4 && indexed.Equals(new Color32(1, 9, 3, 4)), "Color32 indexer and Equals");
    }

    private static void RectBehavior()
    {
        var rect = new Rect(10f, 20f, 30f, 40f);
        Require(rect.xMax == 40f && rect.yMax == 60f && rect.center == new Vector2(25f, 40f), "Rect edges and center");
        rect.xMin = 15f;
        Require(rect.xMax == 40f && rect.width == 25f, "setting xMin keeps xMax");
        rect.xMax = 50f;
        Require(rect.xMin == 15f && rect.width == 35f, "setting xMax keeps xMin");
        rect.yMin = 30f;
        Require(rect.yMax == 60f && rect.height == 30f, "setting yMin keeps yMax");
        rect.yMax = 100f;
        Require(rect.y == 30f && rect.height == 70f, "setting yMax keeps yMin");
        rect.center = new Vector2(0f, 0f);
        Require(rect.position == new Vector2(-17.5f, -35f), "setting the center moves the rectangle");
        rect.min = new Vector2(-20f, -40f);
        Require(rect.max == new Vector2(17.5f, 35f), "setting min keeps max");
        rect.size = new Vector2(1f, 2f);
        rect.position = new Vector2(3f, 4f);
        Require(rect.x == 3f && rect.y == 4f && rect.xMax == 4f && rect.yMax == 6f, "setting size and position");
        var box = new Rect(0f, 0f, 10f, 10f);
        Require(box.Contains(new Vector2(0f, 0f)) && box.Contains(new Vector2(9.999f, 9.999f)), "Contains includes the minimum corner");
        Require(!box.Contains(new Vector2(10f, 5f)) && !box.Contains(new Vector2(5f, 10f)) && !box.Contains(new Vector2(-0.001f, 5f)), "Contains excludes the maximum edges");
        Require(box.Contains(new Vector3(5f, 5f, 99f)), "Contains(Vector3) ignores Z");
        var inverted = new Rect(10f, 10f, -10f, -10f);
        Require(!inverted.Contains(new Vector3(5f, 5f, 0f), false) && inverted.Contains(new Vector3(5f, 5f, 0f), true), "Contains with allowInverse");
        Require(!box.Overlaps(new Rect(10f, 0f, 5f, 5f)) && box.Overlaps(new Rect(9f, 0f, 5f, 5f)), "Overlaps excludes touching edges");
        Require(box.Overlaps(new Rect(9f, 0f, -5f, 5f), true), "Overlaps with allowInverse");
        Require(Rect.MinMaxRect(1f, 2f, 4f, 8f) == new Rect(1f, 2f, 3f, 6f), "MinMaxRect");
        var area = new Rect(10f, 20f, 30f, 40f);
        Require(Rect.NormalizedToPoint(area, new Vector2(0.5f, 0.5f)) == new Vector2(25f, 40f), "NormalizedToPoint");
        Require(Rect.PointToNormalized(area, new Vector2(25f, 40f)) == new Vector2(0.5f, 0.5f), "PointToNormalized");
        Require(area.ToString() == "(x:10.00, y:20.00, width:30.00, height:40.00)", "Rect.ToString");
        Require(Rect.zero.Equals(new Rect(0f, 0f, 0f, 0f)) && area.Equals(new Rect(area)) && area != Rect.zero, "Rect equality and zero");
        Require(new Rect(new Vector2(1f, 2f), new Vector2(3f, 4f)).Equals(new Rect(1f, 2f, 3f, 4f)), "Rect from position and size");
        Require(area.left == 10f && area.right == 40f && area.top == 20f && area.bottom == 60f, "left, right, top and bottom");
    }

    private static void IntegerRectsAndBoxes()
    {
        var cells = new List<Vector2Int>();
        foreach (var cell in new RectInt(0, 0, 3, 2).allPositionsWithin) cells.Add(cell);
        Require(cells.SequenceEqual(new[] { new Vector2Int(0, 0), new Vector2Int(1, 0), new Vector2Int(2, 0), new Vector2Int(0, 1), new Vector2Int(1, 1), new Vector2Int(2, 1) }),
            "RectInt enumerates every cell with x changing fastest");
        Require(new RectInt(-1, -1, 2, 2).allPositionsWithin.ToList().Count == 4 && !new RectInt(0, 0, 0, 5).allPositionsWithin.Any(), "RectInt enumeration counts");
        var rect = new RectInt(0, 0, 10, 10);
        rect.xMin = 4;
        Require(rect.xMax == 10 && rect.width == 6, "RectInt xMin keeps xMax");
        Require(rect.Contains(new Vector2Int(4, 0)) && !rect.Contains(new Vector2Int(10, 5)) && !rect.Contains(new Vector2Int(3, 5)), "RectInt.Contains excludes the maximum edge");
        Require(rect.Overlaps(new RectInt(9, 9, 5, 5)) && !rect.Overlaps(new RectInt(10, 0, 5, 5)), "RectInt.Overlaps");
        var clamped = new RectInt(8, 8, 4, 4);
        clamped.ClampToBounds(new RectInt(0, 0, 10, 10));
        Require(clamped.position == new Vector2Int(6, 6) && clamped.size == new Vector2Int(4, 4), "RectInt.ClampToBounds");
        var negative = new RectInt(5, 0, -3, 1);
        Require(negative.xMin == 2 && negative.xMax == 5, "RectInt orders the edges of a negative width");
        var minMax = new RectInt();
        minMax.SetMinMax(new Vector2Int(1, 2), new Vector2Int(4, 6));
        Require(minMax.position == new Vector2Int(1, 2) && minMax.size == new Vector2Int(3, 4) && minMax.center == new Vector2(2.5f, 4f), "RectInt.SetMinMax and center");
        Require(RectInt.zero == new RectInt(0, 0, 0, 0) && minMax != RectInt.zero, "RectInt equality");

        var box = new BoundsInt(0, 0, 0, 2, 2, 2);
        var positions = box.allPositionsWithin.ToList();
        Require(positions.Count == 8 && positions[0] == new Vector3Int(0, 0, 0) && positions[1] == new Vector3Int(1, 0, 0)
            && positions[2] == new Vector3Int(0, 1, 0) && positions[4] == new Vector3Int(0, 0, 1) && positions[7] == new Vector3Int(1, 1, 1), "BoundsInt enumeration order");
        Require(box.Contains(new Vector3Int(1, 1, 1)) && !box.Contains(new Vector3Int(2, 0, 0)) && !box.Contains(new Vector3Int(0, 0, -1)), "BoundsInt.Contains excludes the maximum faces");
        box.xMin = 1;
        Require(box.xMax == 2 && box.size.x == 1 && box.center == new Vector3(1.5f, 1f, 1f), "BoundsInt xMin keeps xMax");
        box.SetMinMax(new Vector3Int(-1, -1, -1), new Vector3Int(1, 1, 1));
        Require(box.position == new Vector3Int(-1, -1, -1) && box.size == new Vector3Int(2, 2, 2) && box.min == new Vector3Int(-1, -1, -1) && box.max == new Vector3Int(1, 1, 1), "BoundsInt.SetMinMax");
        Require(box.ToString() == "Position: (-1, -1, -1), Size: (2, 2, 2)", "BoundsInt.ToString");
        var inside = new BoundsInt(9, 9, 9, 4, 4, 4);
        inside.ClampToBounds(new BoundsInt(0, 0, 0, 10, 10, 10));
        Require(inside.position == new Vector3Int(6, 6, 6), "BoundsInt.ClampToBounds");
        Require(new BoundsInt(0, 0, 0, 1, 1, 1) == new BoundsInt(Vector3Int.zero, Vector3Int.one) && new BoundsInt(0, 0, 0, 1, 1, 1).Equals(new BoundsInt(0, 0, 0, 1, 1, 1)), "BoundsInt equality");
    }
}
