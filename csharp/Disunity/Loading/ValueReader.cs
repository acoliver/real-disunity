using System.Globalization;
using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Reads Unity's serializable value types from the plan's JSON, in the layouts the project
/// loader and the player decoder both produce. Numbers are strings; integer types parse as integers.</summary>
internal static class ValueReader
{
    private static readonly Dictionary<Type, Func<JsonElement, object>> Readers = new()
    {
        [typeof(Vector2)] = v => new Vector2(Float(v, "x"), Float(v, "y")),
        [typeof(Vector3)] = v => ReadVector3(v),
        [typeof(Vector4)] = v => new Vector4(Float(v, "x"), Float(v, "y"), Float(v, "z"), Float(v, "w")),
        [typeof(Quaternion)] = v => new Quaternion(Float(v, "x"), Float(v, "y"), Float(v, "z"), Float(v, "w")),
        [typeof(Vector2Int)] = v => new Vector2Int(Int(v, "x"), Int(v, "y")),
        [typeof(Vector3Int)] = v => ReadVector3Int(v),
        [typeof(Color)] = v => ReadColor(v),
        [typeof(Color32)] = v => ReadColor32(v),
        [typeof(Rect)] = v => new Rect(Float(v, "x"), Float(v, "y"), Float(v, "width"), Float(v, "height")),
        [typeof(RectInt)] = v => new RectInt(Int(v, "x"), Int(v, "y"), Int(v, "width"), Int(v, "height")),
        [typeof(Bounds)] = v => ReadBounds(v),
        [typeof(BoundsInt)] = v => new BoundsInt(ReadVector3Int(v.GetProperty("m_Position")), ReadVector3Int(v.GetProperty("m_Size"))),
        [typeof(Matrix4x4)] = v => ReadMatrix(v),
        [typeof(LayerMask)] = v => (LayerMask)(int)UInt(v, "m_Bits"),
        [typeof(RenderingLayerMask)] = v => (RenderingLayerMask)UInt(v, "m_Bits"),
        [typeof(AnimationCurve)] = v => ReadCurve(v),
        [typeof(Gradient)] = v => ReadGradient(v),
    };

    /// <summary>The value of a builtin type, or null when <paramref name="type"/> is not one.</summary>
    public static object? Read(JsonElement value, Type type) => Readers.TryGetValue(type, out var read) ? read(value) : null;

    /// <summary>The curve infinity mode a plan stores for <c>m_PreInfinity</c> and <c>m_PostInfinity</c>.
    /// Unity writes 2 for every curve whose ends hold their value, which is how
    /// <c>AnimationCurve.preWrapMode</c> reports <see cref="WrapMode.ClampForever"/>. Unity's other stored
    /// values are not established by any data available here, so they fail the load instead of guessing.</summary>
    internal static WrapMode InfinityMode(int stored) => stored == 2
        ? WrapMode.ClampForever
        : throw new FormatException($"the curve infinity mode {stored} is not one this loader knows");

    private static float Float(JsonElement value, string name) =>
        float.Parse(Text(value.GetProperty(name)), NumberStyles.Float, CultureInfo.InvariantCulture);

    private static int Int(JsonElement value, string name) =>
        int.Parse(Text(value.GetProperty(name)), NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture);

    private static uint UInt(JsonElement value, string name) =>
        uint.Parse(Text(value.GetProperty(name)), NumberStyles.None, CultureInfo.InvariantCulture);

    private static string Text(JsonElement value) => value.GetString() ?? throw new FormatException("expected a value");

    private static Vector3 ReadVector3(JsonElement v) => new(Float(v, "x"), Float(v, "y"), Float(v, "z"));

    private static Vector3Int ReadVector3Int(JsonElement v) => new(Int(v, "x"), Int(v, "y"), Int(v, "z"));

    private static Color ReadColor(JsonElement v) => new(Float(v, "r"), Float(v, "g"), Float(v, "b"), Float(v, "a"));

    /// <summary>The <c>rgba</c> integer packs the bytes little-endian: red in the lowest byte, then green,
    /// blue and alpha, which is the order Unity's <c>Color32</c> has in memory.</summary>
    private static Color32 ReadColor32(JsonElement v)
    {
        var packed = UInt(v, "rgba");
        return new Color32((byte)packed, (byte)(packed >> 8), (byte)(packed >> 16), (byte)(packed >> 24));
    }

    /// <summary>A plan stores a box's half-size as <c>m_Extent</c>.</summary>
    private static Bounds ReadBounds(JsonElement v) =>
        new() { center = ReadVector3(v.GetProperty("m_Center")), extents = ReadVector3(v.GetProperty("m_Extent")) };

    /// <summary>Elements are named <c>mRC</c> for row R and column C, as the fields are.</summary>
    private static Matrix4x4 ReadMatrix(JsonElement v)
    {
        var matrix = new Matrix4x4();
        for (var column = 0; column < 4; column++)
            for (var row = 0; row < 4; row++)
                matrix[row, column] = Float(v, $"m{row}{column}");
        return matrix;
    }

    private static AnimationCurve ReadCurve(JsonElement v)
    {
        var keys = v.GetProperty("m_Curve").EnumerateArray().Select(ReadKeyframe).ToArray();
        return new AnimationCurve(keys)
        {
            preWrapMode = InfinityMode(Int(v, "m_PreInfinity")),
            postWrapMode = InfinityMode(Int(v, "m_PostInfinity")),
        };
    }

    private static Keyframe ReadKeyframe(JsonElement v) => new(Float(v, "time"), Float(v, "value"), Float(v, "inSlope"), Float(v, "outSlope"),
        Float(v, "inWeight"), Float(v, "outWeight")) { weightedMode = (WeightedMode)Int(v, "weightedMode") };

    /// <summary>A gradient stores eight keys. <c>key</c>N holds color key N's rgb and alpha key N's alpha,
    /// with their times in <c>ctime</c>N and <c>atime</c>N as 16-bit fractions of 65535; the key counts say
    /// how many of the eight are in use.</summary>
    private static Gradient ReadGradient(JsonElement v)
    {
        var colorCount = Int(v, "m_NumColorKeys");
        var alphaCount = Int(v, "m_NumAlphaKeys");
        if (colorCount is < 0 or > Gradient.MaxKeys || alphaCount is < 0 or > Gradient.MaxKeys)
            throw new FormatException($"a gradient has at most {Gradient.MaxKeys} keys of each kind, not {colorCount} and {alphaCount}");
        var colors = Enumerable.Range(0, colorCount)
            .Select(i => new GradientColorKey(WithAlpha(ReadColor(v.GetProperty($"key{i}")), 1f), Time(v, $"ctime{i}"))).ToArray();
        var alphas = Enumerable.Range(0, alphaCount)
            .Select(i => new GradientAlphaKey(ReadColor(v.GetProperty($"key{i}")).a, Time(v, $"atime{i}"))).ToArray();
        var gradient = new Gradient { mode = (GradientMode)Int(v, "m_Mode"), colorSpace = (ColorSpace)Int(v, "m_ColorSpace") };
        gradient.SetKeys(colors, alphas);
        return gradient;
    }

    private static Color WithAlpha(Color color, float alpha) => new(color.r, color.g, color.b, alpha);

    private static float Time(JsonElement v, string name) => UInt(v, name) / 65535f;
}
