using System.Globalization;
using System.Reflection;
using UnityEngine;

/// <summary>Known-value checks for Unity's scalar and vector math types. Each expected value is
/// worked out by hand from Unity's documented behaviour, not read back from the engine.</summary>
internal static class MathTests
{
    public static void Run()
    {
        MemberShapes();
        EqualitySemantics();
        ToStringFormats();
        MathfRules();
        MathfInterpolation();
        MathfPowersAndSpaces();
        SmoothDamping();
        Vector3Operations();
        Vector3Interpolation();
        Vector2Operations();
        Vector4AndConversions();
        IntegerVectors();
        Console.WriteLine("PASS: Unity math scalar and vector values (members, equality, formats, Mathf, Vector2/3/4, integer vectors)");
    }

    internal static void Require(bool condition, string message)
    {
        if (!condition) throw new Exception("math: " + message);
    }

    internal static bool Near(float a, float b, float tolerance = 1e-4f) => MathF.Abs(a - b) <= tolerance;

    internal static bool Near(Vector3 a, Vector3 b, float tolerance = 1e-4f) =>
        Near(a.x, b.x, tolerance) && Near(a.y, b.y, tolerance) && Near(a.z, b.z, tolerance);

    internal static void RequireNear(Vector3 actual, Vector3 expected, string message, float tolerance = 1e-4f) =>
        Require(Near(actual, expected, tolerance), $"{message}: expected {expected.ToString("F4")} but got {actual.ToString("F4")}");

    internal static void RequireNear(float actual, float expected, string message, float tolerance = 1e-4f) =>
        Require(Near(actual, expected, tolerance), $"{message}: expected {expected} but got {actual}");

    private static void MemberShapes()
    {
        Require(typeof(Vector3).GetField("x") is { IsStatic: false }, "Vector3.x is an instance field");
        Require(typeof(Vector3).GetProperty("x") is null, "Vector3.x is not a property");
        Require(typeof(Color).GetField("a") is { IsStatic: false }, "Color.a is an instance field");
        Require(typeof(Color).GetProperty("a") is null, "Color.a is not a property");
        Require(typeof(Quaternion).GetField("w") is not null, "Quaternion.w is a field");
        Require(typeof(Matrix4x4).GetField("m23") is not null, "Matrix4x4.m23 is a field");
        Require(typeof(Color32).GetField("r")?.FieldType == typeof(byte), "Color32.r is a byte field");
        Require(typeof(Mathf).GetField("PI") is { IsLiteral: true }, "Mathf.PI is a constant");
        Require(typeof(Vector3).GetField("kEpsilon") is { IsLiteral: true }, "Vector3.kEpsilon is a constant");
        Require(typeof(Rect).GetProperty("zero") is { GetMethod.IsStatic: true }, "Rect.zero is a static property");
        Require(typeof(Rect).GetProperty("x") is not null && typeof(Rect).GetField("x") is null, "Rect.x is a property");
        Require(typeof(Vector2Int).GetProperty("x") is not null && typeof(Vector2Int).GetField("x") is null, "Vector2Int.x is a property");
        Require(typeof(Mathf).GetMethod("SmoothDamp", new[] { typeof(float), typeof(float), typeof(float).MakeByRefType(), typeof(float) }) is not null,
            "Mathf.SmoothDamp has a four argument overload");
        Require(new Color(1, 2, 3).a == 1f, "three argument Color constructor sets alpha to one");
        Require(Marshal(typeof(Vector2)) == 8 && Marshal(typeof(Vector3)) == 12 && Marshal(typeof(Quaternion)) == 16
            && Marshal(typeof(Color)) == 16 && Marshal(typeof(Matrix4x4)) == 64 && Marshal(typeof(Color32)) == 4,
            "native-facing structs keep their field-for-field sizes");
    }

    private static int Marshal(Type type) => System.Runtime.InteropServices.Marshal.SizeOf(type);

    private static void EqualitySemantics()
    {
        var v = new Vector3(1f, 2f, 3f);
        var close = new Vector3(1.000001f, 2f, 3f);
        Require(v == close && !(v != close), "Vector3 == is approximate below 1e-5");
        Require(!v.Equals(close), "Vector3.Equals is exact");
        Require(!(Vector3.zero == new Vector3(0.00002f, 0f, 0f)), "Vector3 == fails above 1e-5");
        Require(new Vector2(1f, 2f) == new Vector2(1f, 2.000001f) && !new Vector2(1f, 2f).Equals(new Vector2(1f, 2.000001f)), "Vector2 equality");
        Require(new Vector4(1f, 2f, 3f, 4f) == new Vector4(1f, 2f, 3f, 4.000001f), "Vector4 == is approximate");
        Require(!new Vector4(1f, 2f, 3f, 4f).Equals(new Vector4(1f, 2f, 3f, 4.000001f)), "Vector4.Equals is exact");
        Require(new Vector3(float.NaN, 0f, 0f).Equals(new Vector3(float.NaN, 0f, 0f)), "Equals treats NaN as equal to itself");

        var q = Quaternion.Euler(10f, 20f, 30f);
        var negated = new Quaternion(-q.x, -q.y, -q.z, -q.w);
        var copy = q;
        Require(q == copy && !(q != copy), "a quaternion equals its copy");
        Require(!(q == negated) && q != negated, "a quaternion does not equal its negation");
        Require(q == new Quaternion(q.x + 1e-7f, q.y, q.z, q.w), "Quaternion == tolerates 1e-7");
        Require(!q.Equals(new Quaternion(q.x + 1e-7f, q.y, q.z, q.w)), "Quaternion.Equals is exact");

        Require(new Color(1f, 0f, 0f, 1f) == new Color(1.000001f, 0f, 0f, 1f), "Color == is approximate");
        Require(!new Color(1f, 0f, 0f, 1f).Equals(new Color(1.000001f, 0f, 0f, 1f)), "Color.Equals is exact");
        Require(new Vector3(1f, 2f, 3f).GetHashCode() == new Vector3(1f, 2f, 3f).GetHashCode(), "equal vectors hash alike");
        Require(new Vector2Int(1, 2) == new Vector2Int(1, 2) && new Vector2Int(1, 2) != new Vector2Int(2, 1), "Vector2Int equality");
        Require(new Vector2Int(3, 4).GetHashCode() == new Vector2Int(3, 4).GetHashCode(), "equal integer vectors hash alike");
        var set = new HashSet<Vector3Int> { new(1, 2, 3), new(1, 2, 3), new(3, 2, 1) };
        Require(set.Count == 2, "Vector3Int works as a set key");
    }

    private static void ToStringFormats()
    {
        Require(new Vector3(1f, 2.346f, -3f).ToString() == "(1.00, 2.35, -3.00)", "Vector3.ToString uses F2");
        Require(new Vector2(0.5f, -1.256f).ToString() == "(0.50, -1.26)", "Vector2.ToString uses F2");
        Require(new Vector4(1f, 2f, 3f, 4f).ToString() == "(1.00, 2.00, 3.00, 4.00)", "Vector4.ToString uses F2");
        Require(Quaternion.identity.ToString() == "(0.00000, 0.00000, 0.00000, 1.00000)", "Quaternion.ToString uses F5");
        Require(new Color(1f, 0.5f, 0.25f, 1f).ToString() == "RGBA(1.000, 0.500, 0.250, 1.000)", "Color.ToString uses F3");
        Require(new Color32(255, 128, 0, 255).ToString() == "RGBA(255, 128, 0, 255)", "Color32.ToString prints integers");
        Require(Matrix4x4.identity.ToString() == "1.00000\t0.00000\t0.00000\t0.00000\n0.00000\t1.00000\t0.00000\t0.00000\n0.00000\t0.00000\t1.00000\t0.00000\n0.00000\t0.00000\t0.00000\t1.00000\n",
            "Matrix4x4.ToString prints F5 rows");
        Require(new Bounds(new Vector3(1f, 2f, 3f), new Vector3(2f, 4f, 6f)).ToString() == "Center: (1.0, 2.0, 3.0), Extents: (1.0, 2.0, 3.0)", "Bounds.ToString");
        Require(new Plane(Vector3.up, Vector3.up).ToString() == "(normal:(0.0, 1.0, 0.0), distance:-1.0)", "Plane.ToString");
        Require(new Ray(Vector3.zero, new Vector3(0f, 0f, 2f)).ToString() == "Origin: (0.0, 0.0, 0.0), Dir: (0.0, 0.0, 1.0)", "Ray.ToString");
        Require(new Vector2Int(1, -2).ToString() == "(1, -2)" && new Vector3Int(1, -2, 3).ToString() == "(1, -2, 3)", "integer vector ToString");
        Require(new RectInt(1, 2, 3, 4).ToString() == "(x:1, y:2, width:3, height:4)", "RectInt.ToString");
        Require(new Vector3(1.5f, 2f, 3f).ToString("F0") == "(2, 2, 3)", "ToString honours a format");
        var commaFormat = new NumberFormatInfo { NumberDecimalSeparator = "," };
        Require(new Vector3(1.5f, 2f, 3f).ToString("F1", commaFormat) == "(1,5, 2,0, 3,0)", "ToString honours a format provider");
        var previous = CultureInfo.CurrentCulture;
        var comma = (CultureInfo)CultureInfo.InvariantCulture.Clone();
        comma.NumberFormat.NumberDecimalSeparator = ",";
        CultureInfo.CurrentCulture = comma;
        var customCulture = new Vector3(1.5f, 2f, 3f).ToString();
        CultureInfo.CurrentCulture = previous;
        Require(customCulture == "(1.50, 2.00, 3.00)", "default ToString ignores the current culture");
    }

    private static void MathfRules()
    {
        RequireNear(Mathf.Clamp(5f, 10f, 0f), 10f, "Clamp with min above max returns min for a low value");
        RequireNear(Mathf.Clamp(20f, 10f, 0f), 0f, "Clamp with min above max returns max for a high value");
        Require(Mathf.Clamp(5, 10, 0) == 10 && Mathf.Clamp(20, 10, 0) == 0 && Mathf.Clamp(5, 0, 10) == 5, "integer Clamp");
        Require(Mathf.Round(0.5f) == 0f && Mathf.Round(1.5f) == 2f && Mathf.Round(2.5f) == 2f, "Round rounds half to even");
        Require(Mathf.RoundToInt(-0.5f) == 0 && Mathf.RoundToInt(-1.5f) == -2 && Mathf.RoundToInt(3.5f) == 4, "RoundToInt rounds half to even");
        Require(Mathf.FloorToInt(-0.5f) == -1 && Mathf.CeilToInt(-0.5f) == 0 && Mathf.CeilToInt(0.1f) == 1, "Floor and Ceil to int");
        Require(Mathf.Sign(0f) == 1f && Mathf.Sign(-0f) == 1f && Mathf.Sign(-3f) == -1f, "Sign of zero is one");
        Require(Mathf.Approximately(1f, 1.0000005f) && !Mathf.Approximately(1f, 1.00001f), "Approximately scales with magnitude");
        Require(Mathf.Approximately(0f, 0f) && !Mathf.Approximately(0f, 1e-8f), "Approximately near zero");
        Require(Mathf.Epsilon == float.Epsilon, "Epsilon is the smallest float");
        Require(Mathf.Min(3, 1, 2) == 1 && Mathf.Max(3f, 1f, 2f) == 3f && Mathf.Min(new int[0]) == 0, "params Min and Max");
        Require(Mathf.Repeat(-1f, 3f) == 2f && Mathf.Repeat(7f, 3f) == 1f && Mathf.Repeat(-3f, 3f) == 0f, "Repeat with negatives");
        RequireNear(Mathf.PingPong(4f, 3f), 2f, "PingPong past the end");
        RequireNear(Mathf.PingPong(-1f, 3f), 1f, "PingPong of a negative");
        Require(Mathf.PingPong(0f, 3f) == 0f && Mathf.PingPong(3f, 3f) == 3f, "PingPong at the ends");
        RequireNear(Mathf.DeltaAngle(350f, 10f), 20f, "DeltaAngle across zero");
        RequireNear(Mathf.DeltaAngle(10f, 350f), -20f, "DeltaAngle backwards across zero");
        RequireNear(Mathf.DeltaAngle(0f, -90f), -90f, "DeltaAngle of a negative target");
        RequireNear(Mathf.DeltaAngle(-170f, 170f), -20f, "DeltaAngle across minus 180");
        Require(Mathf.Abs(-2.5f) == 2.5f && Mathf.Abs(-4) == 4 && Mathf.Sqrt(9f) == 3f, "Abs and Sqrt");
        RequireNear(Mathf.Rad2Deg * Mathf.Deg2Rad, 1f, "Deg2Rad and Rad2Deg are inverses", 1e-6f);
        RequireNear(Mathf.Gamma(0.5f, 1f, 2f), 0.25f, "Gamma curve");
        RequireNear(Mathf.Gamma(-0.5f, 1f, 2f), -0.25f, "Gamma keeps the sign");
    }

    private static void MathfInterpolation()
    {
        Require(Mathf.Lerp(0f, 10f, 2f) == 10f && Mathf.Lerp(0f, 10f, -1f) == 0f, "Lerp clamps t");
        Require(Mathf.LerpUnclamped(0f, 10f, 2f) == 20f, "LerpUnclamped does not clamp t");
        Require(Mathf.InverseLerp(5f, 5f, 7f) == 0f, "InverseLerp of an empty range is zero");
        Require(Mathf.InverseLerp(0f, 10f, 5f) == 0.5f && Mathf.InverseLerp(0f, 10f, 20f) == 1f, "InverseLerp clamps");
        RequireNear(Mathf.LerpAngle(350f, 10f, 0.5f), 360f, "LerpAngle takes the short way");
        Require(Mathf.MoveTowards(0f, 10f, 3f) == 3f && Mathf.MoveTowards(0f, 10f, 30f) == 10f && Mathf.MoveTowards(5f, 0f, 2f) == 3f, "MoveTowards");
        RequireNear(Mathf.MoveTowardsAngle(350f, 10f, 5f), 355f, "MoveTowardsAngle across zero");
        RequireNear(Mathf.MoveTowardsAngle(350f, 10f, 50f), 10f, "MoveTowardsAngle snaps to the target");
        RequireNear(Mathf.SmoothStep(0f, 10f, 0.5f), 5f, "SmoothStep midpoint");
        RequireNear(Mathf.SmoothStep(0f, 1f, 0.25f), 0.15625f, "SmoothStep quarter");
        Require(Mathf.Clamp01(-1f) == 0f && Mathf.Clamp01(2f) == 1f && Mathf.Clamp01(0.3f) == 0.3f, "Clamp01");
    }

    private static void MathfPowersAndSpaces()
    {
        Require(Mathf.NextPowerOfTwo(0) == 0 && Mathf.NextPowerOfTwo(1) == 1 && Mathf.NextPowerOfTwo(3) == 4
            && Mathf.NextPowerOfTwo(8) == 8 && Mathf.NextPowerOfTwo(9) == 16, "NextPowerOfTwo");
        Require(Mathf.ClosestPowerOfTwo(5) == 4 && Mathf.ClosestPowerOfTwo(7) == 8 && Mathf.ClosestPowerOfTwo(6) == 8
            && Mathf.ClosestPowerOfTwo(12) == 16, "ClosestPowerOfTwo, ties going up");
        Require(Mathf.IsPowerOfTwo(64) && !Mathf.IsPowerOfTwo(63) && Mathf.IsPowerOfTwo(1), "IsPowerOfTwo");
        RequireNear(Mathf.GammaToLinearSpace(0.5f), 0.214041f, "sRGB to linear", 1e-5f);
        RequireNear(Mathf.GammaToLinearSpace(0.02f), 0.02f / 12.92f, "sRGB to linear in the linear segment", 1e-7f);
        RequireNear(Mathf.LinearToGammaSpace(0.214041f), 0.5f, "linear to sRGB", 1e-5f);
        RequireNear(Mathf.LinearToGammaSpace(Mathf.GammaToLinearSpace(0.8f)), 0.8f, "gamma round trip", 1e-5f);
        Require(Mathf.FloatToHalf(1f) == 0x3C00 && Mathf.FloatToHalf(-2f) == 0xC000, "FloatToHalf bit patterns");
        Require(Mathf.HalfToFloat(0x3C00) == 1f, "HalfToFloat of one");
        RequireNear(Mathf.HalfToFloat(0x3555), 0.33325195f, "HalfToFloat of one third", 1e-7f);
        var kelvin = Mathf.CorrelatedColorTemperatureToRGB(6500f);
        Require(kelvin.r > 0.95f && kelvin.g > 0.95f && kelvin.b > 0.95f && kelvin.a == 1f, "daylight temperature is near white");
        Require(Mathf.CorrelatedColorTemperatureToRGB(2000f).b < Mathf.CorrelatedColorTemperatureToRGB(2000f).r, "warm light is red");
        Perlin();
    }

    private static void Perlin()
    {
        RequireNear(Mathf.PerlinNoise(3f, 7f), 0.5f, "Perlin noise is 0.5 on the lattice", 1e-6f);
        var distinct = new HashSet<float>();
        for (var i = 0; i < 200; i++)
        {
            var x = i * 0.137f;
            var y = i * 0.291f;
            var value = Mathf.PerlinNoise(x, y);
            Require(value >= 0f && value <= 1f, "Perlin noise stays in [0, 1]");
            Require(MathF.Abs(Mathf.PerlinNoise(x + 0.001f, y) - value) < 0.02f, "Perlin noise is continuous");
            Require(Mathf.PerlinNoise(x, y) == value, "Perlin noise is deterministic");
            var line = Mathf.PerlinNoise1D(x);
            Require(line >= 0f && line <= 1f, "1D Perlin noise stays in [0, 1]");
            distinct.Add(MathF.Round(value, 2));
        }
        Require(distinct.Count > 10, "Perlin noise varies");
    }

    private static void SmoothDamping()
    {
        var position = 0f;
        var velocity = 0f;
        var previous = 0f;
        for (var step = 0; step < 400; step++)
        {
            position = Mathf.SmoothDamp(position, 10f, ref velocity, 0.3f, Mathf.Infinity, 0.02f);
            Require(position <= 10.0001f && position >= previous - 1e-4f, "SmoothDamp approaches the target monotonically");
            previous = position;
        }
        RequireNear(position, 10f, "SmoothDamp converges", 0.01f);

        var fast = 9.9f;
        var fastVelocity = 100f;
        var clamped = Mathf.SmoothDamp(fast, 10f, ref fastVelocity, 0.3f, Mathf.Infinity, 0.02f);
        Require(clamped == 10f && fastVelocity == 0f, "SmoothDamp stops at the target instead of overshooting");

        var limited = 0f;
        var limitedVelocity = 0f;
        for (var step = 0; step < 300; step++)
        {
            limited = Mathf.SmoothDamp(limited, 10f, ref limitedVelocity, 0.3f, 1f, 0.02f);
            Require(MathF.Abs(limitedVelocity) <= 1.05f, "SmoothDamp respects maxSpeed");
        }
        Require(limited > 1f && limited < 10f, "maxSpeed slows the approach");

        var angle = 350f;
        var angleVelocity = 0f;
        for (var step = 0; step < 400; step++)
            angle = Mathf.SmoothDampAngle(angle, 10f, ref angleVelocity, 0.3f, Mathf.Infinity, 0.02f);
        RequireNear(Mathf.DeltaAngle(angle, 10f), 0f, "SmoothDampAngle goes the short way round", 0.01f);

        var vector = Vector3.zero;
        var vectorVelocity = Vector3.zero;
        for (var step = 0; step < 400; step++)
        {
            vector = Vector3.SmoothDamp(vector, new Vector3(4f, 0f, 3f), ref vectorVelocity, 0.3f, Mathf.Infinity, 0.02f);
            Require(vector.magnitude <= 5.001f, "Vector3.SmoothDamp does not overshoot");
        }
        RequireNear(vector, new Vector3(4f, 0f, 3f), "Vector3.SmoothDamp converges", 0.01f);

        var flat = Vector2.zero;
        var flatVelocity = Vector2.zero;
        for (var step = 0; step < 400; step++)
            flat = Vector2.SmoothDamp(flat, new Vector2(3f, 4f), ref flatVelocity, 0.3f, Mathf.Infinity, 0.02f);
        Require(Vector2.Distance(flat, new Vector2(3f, 4f)) < 0.01f, "Vector2.SmoothDamp converges");
    }

    private static void Vector3Operations()
    {
        RequireNear(Vector3.Cross(Vector3.right, Vector3.up), Vector3.forward, "Cross of right and up");
        RequireNear(Vector3.Cross(Vector3.up, Vector3.right), Vector3.back, "Cross is anti-commutative");
        RequireNear(Vector3.Dot(new Vector3(1f, 2f, 3f), new Vector3(4f, -5f, 6f)), 12f, "Dot");
        RequireNear(new Vector3(3f, 4f, 0f).magnitude, 5f, "magnitude");
        RequireNear(new Vector3(3f, 4f, 12f).sqrMagnitude, 169f, "sqrMagnitude");
        Require(new Vector3(0.000001f, 0f, 0f).normalized.Equals(Vector3.zero), "normalizing a tiny vector gives zero");
        RequireNear(new Vector3(0f, 0f, 9f).normalized, Vector3.forward, "normalized");
        RequireNear(Vector3.Angle(Vector3.right, Vector3.up), 90f, "Angle between perpendicular vectors");
        RequireNear(Vector3.Angle(Vector3.zero, Vector3.right), 0f, "Angle with a zero vector");
        RequireNear(Vector3.Angle(Vector3.right, new Vector3(2f, 0f, 0f)), 0f, "Angle of parallel vectors", 0.05f);
        RequireNear(Vector3.SignedAngle(Vector3.right, Vector3.forward, Vector3.up), -90f, "SignedAngle about up");
        RequireNear(Vector3.SignedAngle(Vector3.forward, Vector3.right, Vector3.up), 90f, "SignedAngle the other way");
        RequireNear(Vector3.Reflect(new Vector3(1f, -1f, 0f), Vector3.up), new Vector3(1f, 1f, 0f), "Reflect");
        RequireNear(Vector3.Project(new Vector3(2f, 3f, 0f), Vector3.right), new Vector3(2f, 0f, 0f), "Project");
        RequireNear(Vector3.Project(Vector3.one, Vector3.zero), Vector3.zero, "Project onto zero");
        RequireNear(Vector3.ProjectOnPlane(Vector3.one, Vector3.up), new Vector3(1f, 0f, 1f), "ProjectOnPlane");
        RequireNear(Vector3.ClampMagnitude(new Vector3(3f, 4f, 0f), 2.5f), new Vector3(1.5f, 2f, 0f), "ClampMagnitude shortens");
        RequireNear(Vector3.ClampMagnitude(new Vector3(3f, 4f, 0f), 9f), new Vector3(3f, 4f, 0f), "ClampMagnitude keeps short vectors");
        RequireNear(Vector3.MoveTowards(Vector3.zero, new Vector3(10f, 0f, 0f), 3f), new Vector3(3f, 0f, 0f), "MoveTowards");
        RequireNear(Vector3.MoveTowards(Vector3.zero, new Vector3(1f, 0f, 0f), 3f), new Vector3(1f, 0f, 0f), "MoveTowards arrives");
        RequireNear(Vector3.Distance(Vector3.one, new Vector3(1f, 1f, 4f)), 3f, "Distance");
        RequireNear(Vector3.Scale(new Vector3(1f, 2f, 3f), new Vector3(2f, 3f, 4f)), new Vector3(2f, 6f, 12f), "Scale");
        RequireNear(Vector3.Min(new Vector3(1f, 5f, 3f), new Vector3(2f, 4f, 3f)), new Vector3(1f, 4f, 3f), "Min");
        RequireNear(Vector3.Max(new Vector3(1f, 5f, 3f), new Vector3(2f, 4f, 3f)), new Vector3(2f, 5f, 3f), "Max");
        Require(Vector3.forward.Equals(new Vector3(0f, 0f, 1f)) && Vector3.back.Equals(new Vector3(0f, 0f, -1f))
            && Vector3.left.Equals(new Vector3(-1f, 0f, 0f)) && Vector3.down.Equals(new Vector3(0f, -1f, 0f)), "direction constants");
        Require(float.IsPositiveInfinity(Vector3.positiveInfinity.x) && float.IsNegativeInfinity(Vector3.negativeInfinity.z), "infinity constants");
        var normal = new Vector3(2f, 0f, 0f);
        var tangent = new Vector3(1f, 1f, 0f);
        Vector3.OrthoNormalize(ref normal, ref tangent);
        RequireNear(normal, Vector3.right, "OrthoNormalize normal");
        RequireNear(tangent, Vector3.up, "OrthoNormalize tangent");
        var indexed = new Vector3(1f, 2f, 3f);
        indexed[1] = 7f;
        Require(indexed[0] == 1f && indexed[1] == 7f && indexed[2] == 3f, "Vector3 indexer reads and writes");
        try
        {
            _ = indexed[3];
            Require(false, "Vector3 indexer must reject index 3");
        }
        catch (IndexOutOfRangeException error)
        {
            Require(error.Message == "Invalid Vector3 index!", "Vector3 index error message");
        }
        var scaled = new Vector3(1f, 2f, 3f);
        scaled.Scale(new Vector3(2f, 2f, 2f));
        scaled.Set(scaled.x, scaled.y, 0f);
        Require(scaled.Equals(new Vector3(2f, 4f, 0f)), "instance Scale and Set");
        var shortened = new Vector3(0f, 0f, 5f);
        shortened.Normalize();
        Require(shortened.Equals(Vector3.forward), "instance Normalize");
        RequireNear((new Vector3(1f, 2f, 3f) * 2f + new Vector3(1f, 1f, 1f) - 3f * Vector3.one) / 2f, new Vector3(0f, 1f, 2f), "arithmetic operators");
        RequireNear(-new Vector3(1f, -2f, 3f), new Vector3(-1f, 2f, -3f), "negation");
    }

    private static void Vector3Interpolation()
    {
        RequireNear(Vector3.Lerp(Vector3.zero, new Vector3(10f, 0f, 0f), 2f), new Vector3(10f, 0f, 0f), "Lerp clamps");
        RequireNear(Vector3.LerpUnclamped(Vector3.zero, new Vector3(10f, 0f, 0f), 2f), new Vector3(20f, 0f, 0f), "LerpUnclamped");
        RequireNear(Vector3.Slerp(Vector3.right, Vector3.up, 0.5f), new Vector3(0.70710678f, 0.70710678f, 0f), "Slerp of unit vectors");
        RequireNear(Vector3.Slerp(new Vector3(2f, 0f, 0f), new Vector3(0f, 4f, 0f), 0.5f), new Vector3(2.1213203f, 2.1213203f, 0f), "Slerp interpolates length");
        var opposite = Vector3.Slerp(Vector3.right, Vector3.left, 0.5f);
        RequireNear(opposite.magnitude, 1f, "Slerp of opposite vectors keeps length");
        RequireNear(Vector3.Dot(opposite, Vector3.right), 0f, "Slerp of opposite vectors turns through a right angle");
        RequireNear(Vector3.RotateTowards(Vector3.right, Vector3.up, MathF.PI / 4f, 0f), new Vector3(0.70710678f, 0.70710678f, 0f), "RotateTowards by a quarter turn");
        RequireNear(Vector3.RotateTowards(Vector3.right, Vector3.up, 10f, 0f), Vector3.up, "RotateTowards arrives");
        RequireNear(Vector3.RotateTowards(Vector3.right, new Vector3(0f, 3f, 0f), 10f, 1f), new Vector3(0f, 2f, 0f), "RotateTowards moves length");
    }

    private static void Vector2Operations()
    {
        RequireNear(Vector2.Perpendicular(Vector2.right).x, 0f, "Perpendicular x");
        RequireNear(Vector2.Perpendicular(Vector2.right).y, 1f, "Perpendicular y");
        RequireNear(Vector2.SignedAngle(Vector2.right, Vector2.up), 90f, "SignedAngle counter-clockwise");
        RequireNear(Vector2.SignedAngle(Vector2.up, Vector2.right), -90f, "SignedAngle clockwise");
        RequireNear(Vector2.Angle(Vector2.right, new Vector2(1f, 1f)), 45f, "Angle");
        RequireNear(new Vector2(3f, 4f).magnitude, 5f, "magnitude");
        Require(new Vector2(0f, 0f).normalized.Equals(Vector2.zero), "normalizing zero gives zero");
        Require((new Vector2(0f, 7f).normalized - Vector2.up).magnitude < 1e-5f, "normalized");
        Require(Vector2.Reflect(new Vector2(1f, -1f), Vector2.up) == new Vector2(1f, 1f), "Reflect");
        Require(Vector2.ClampMagnitude(new Vector2(3f, 4f), 2.5f) == new Vector2(1.5f, 2f), "ClampMagnitude");
        Require(Vector2.MoveTowards(Vector2.zero, new Vector2(10f, 0f), 4f) == new Vector2(4f, 0f), "MoveTowards");
        Require(Vector2.Scale(new Vector2(2f, 3f), new Vector2(4f, 5f)) == new Vector2(8f, 15f), "Scale");
        Require(new Vector2(2f, 8f) / new Vector2(2f, 4f) == new Vector2(1f, 2f) && new Vector2(2f, 3f) * new Vector2(2f, 2f) == new Vector2(4f, 6f), "componentwise operators");
        Require(Vector2.Lerp(Vector2.zero, Vector2.one, 5f) == Vector2.one, "Lerp clamps");
        Require(Vector2.Min(new Vector2(1f, 5f), new Vector2(2f, 4f)) == new Vector2(1f, 4f), "Min");
        Vector3 widened = new Vector2(1f, 2f);
        Vector2 narrowed = new Vector3(1f, 2f, 3f);
        Require(widened.Equals(new Vector3(1f, 2f, 0f)) && narrowed.Equals(new Vector2(1f, 2f)), "Vector2 and Vector3 convert implicitly");
        var indexed = new Vector2(1f, 2f);
        indexed[0] = 9f;
        Require(indexed.x == 9f && indexed[1] == 2f, "Vector2 indexer");
        Require(Vector2.down.Equals(new Vector2(0f, -1f)) && Vector2.left.Equals(new Vector2(-1f, 0f)), "Vector2 direction constants");
    }

    private static void Vector4AndConversions()
    {
        Vector4 fromVector3 = new Vector3(1f, 2f, 3f);
        Vector3 backToVector3 = new Vector4(1f, 2f, 3f, 4f);
        Vector4 fromVector2 = new Vector2(1f, 2f);
        Require(fromVector3.Equals(new Vector4(1f, 2f, 3f, 0f)) && backToVector3.Equals(new Vector3(1f, 2f, 3f)) && fromVector2.Equals(new Vector4(1f, 2f, 0f, 0f)),
            "Vector4 converts to and from Vector2 and Vector3");
        RequireNear(Vector4.Dot(new Vector4(1f, 2f, 3f, 4f), new Vector4(2f, 0f, 1f, 1f)), 9f, "Vector4 Dot");
        RequireNear(new Vector4(1f, 2f, 2f, 4f).magnitude, 5f, "Vector4 magnitude");
        Require(new Vector4(0f, 0f, 0f, 3f).normalized.Equals(new Vector4(0f, 0f, 0f, 1f)), "Vector4 normalized");
        Require(Vector4.Project(new Vector4(2f, 3f, 0f, 0f), new Vector4(1f, 0f, 0f, 0f)) == new Vector4(2f, 0f, 0f, 0f), "Vector4 Project");
        Require(Vector4.Lerp(Vector4.zero, Vector4.one, 0.5f) == new Vector4(0.5f, 0.5f, 0.5f, 0.5f), "Vector4 Lerp");
        Require(new Vector4(1f, 2f, 3f).w == 0f && new Vector4(1f, 2f).z == 0f, "short Vector4 constructors zero the rest");
        var indexed = new Vector4(1f, 2f, 3f, 4f);
        indexed[3] = 8f;
        Require(indexed.w == 8f && indexed[2] == 3f, "Vector4 indexer");
        Vector4 asVector = new Color(0.1f, 0.2f, 0.3f, 0.4f);
        Color asColor = new Vector4(0.5f, 0.6f, 0.7f, 0.8f);
        Require(asVector.Equals(new Vector4(0.1f, 0.2f, 0.3f, 0.4f)) && asColor.Equals(new Color(0.5f, 0.6f, 0.7f, 0.8f)), "Color converts to and from Vector4");
    }

    private static void IntegerVectors()
    {
        Require(Vector2Int.FloorToInt(new Vector2(-0.5f, -1.5f)) == new Vector2Int(-1, -2), "Vector2Int FloorToInt on negatives");
        Require(Vector2Int.CeilToInt(new Vector2(-0.5f, -1.5f)) == new Vector2Int(0, -1), "Vector2Int CeilToInt on negatives");
        Require(Vector2Int.RoundToInt(new Vector2(0.5f, 2.5f)) == new Vector2Int(0, 2), "Vector2Int RoundToInt rounds half to even");
        Require(Vector3Int.FloorToInt(new Vector3(-0.1f, 0.9f, -2f)) == new Vector3Int(-1, 0, -2), "Vector3Int FloorToInt on negatives");
        Require(Vector3Int.CeilToInt(new Vector3(-0.1f, 0.1f, 2f)) == new Vector3Int(0, 1, 2), "Vector3Int CeilToInt");
        Require(Vector3Int.RoundToInt(new Vector3(1.5f, -1.5f, 0.4f)) == new Vector3Int(2, -2, 0), "Vector3Int RoundToInt");
        Require(new Vector2Int(-3, 3) / 2 == new Vector2Int(-1, 1) && new Vector3Int(-3, 3, 5) / 2 == new Vector3Int(-1, 1, 2),
            "integer vector division truncates toward zero");
        Require(new Vector2Int(1, 2) + new Vector2Int(3, 4) == new Vector2Int(4, 6) && new Vector2Int(3, 4) - new Vector2Int(1, 1) == new Vector2Int(2, 3), "Vector2Int +, -");
        Require(2 * new Vector2Int(1, 2) == new Vector2Int(2, 4) && new Vector2Int(1, 2) * 3 == new Vector2Int(3, 6) && -new Vector2Int(1, -2) == new Vector2Int(-1, 2), "Vector2Int scaling");
        RequireNear(Vector2Int.Distance(new Vector2Int(0, 0), new Vector2Int(3, 4)), 5f, "Vector2Int Distance");
        Require(new Vector2Int(3, 4).sqrMagnitude == 25 && new Vector3Int(1, 2, 2).sqrMagnitude == 9, "integer sqrMagnitude");
        var clamped = new Vector2Int(15, -5);
        clamped.Clamp(new Vector2Int(0, 0), new Vector2Int(10, 10));
        Require(clamped == new Vector2Int(10, 0), "Vector2Int Clamp");
        var clamped3 = new Vector3Int(15, -5, 5);
        clamped3.Clamp(Vector3Int.zero, new Vector3Int(10, 10, 10));
        Require(clamped3 == new Vector3Int(10, 0, 5), "Vector3Int Clamp");
        Require(Vector2Int.Min(new Vector2Int(1, 5), new Vector2Int(2, 4)) == new Vector2Int(1, 4) && Vector3Int.Max(new Vector3Int(1, 5, 0), new Vector3Int(2, 4, 3)) == new Vector3Int(2, 5, 3), "integer Min and Max");
        Require(Vector2Int.Scale(new Vector2Int(2, 3), new Vector2Int(4, 5)) == new Vector2Int(8, 15), "Vector2Int Scale");
        Vector2 widened = new Vector2Int(1, 2);
        Vector3 widened3 = new Vector3Int(1, 2, 3);
        var narrowed = (Vector2Int)new Vector3Int(1, 2, 3);
        var raised = (Vector3Int)new Vector2Int(1, 2);
        Require(widened.Equals(new Vector2(1f, 2f)) && widened3.Equals(new Vector3(1f, 2f, 3f)) && narrowed == new Vector2Int(1, 2) && raised == new Vector3Int(1, 2, 0), "integer vector conversions");
        var indexed = new Vector3Int(1, 2, 3);
        indexed[2] = 9;
        Require(indexed.z == 9 && indexed[0] == 1, "Vector3Int indexer");
        Require(Vector3Int.forward == new Vector3Int(0, 0, 1) && Vector2Int.down == new Vector2Int(0, -1) && Vector3Int.one == new Vector3Int(1, 1, 1), "integer direction constants");
    }
}
