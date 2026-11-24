namespace UnityEngine;

/// <summary>
/// The one place where a color property meets the project's color space. A color a game sets (a material's, a property block's or
/// a global) is kept in the vector slot as the shader reads it: converted to linear, RGB only, when
/// <see cref="QualitySettings.activeColorSpace"/> is Linear, and as it was set in Gamma space. The matching getter converts it back,
/// so a game that sets a color and reads it gets its own value. A vector is never converted. A color a plan stores with a material
/// is set through the same setters, so it follows the same rule.
/// </summary>
internal sealed partial class PropertyStore
{
    private static bool InLinearSpace => QualitySettings.activeColorSpace == ColorSpace.Linear;

    /// <summary>The value the shader reads for <paramref name="color"/>.</summary>
    internal static Color ToShader(Color color) => InLinearSpace ? color.linear : color;

    /// <summary>The color a game set, from the value the shader reads.</summary>
    internal static Color FromShader(Color stored) => InLinearSpace ? stored.gamma : stored;

    internal void SetColor(int id, Color color) => Vectors[id] = ToShader(color);

    internal Color GetColor(int id) => FromShader(Vectors.GetValueOrDefault(id));

    internal void SetColorArray(int id, Color[]? values, string member) => ColorArrays[id] = MapColors(PropertyStore.Copy(values, member), ToShader);

    internal void SetColorArray(int id, List<Color>? values, string member) => ColorArrays[id] = MapColors(PropertyStore.Copy(values, member), ToShader);

    internal Color[]? GetColorArray(int id) => ColorArrays.TryGetValue(id, out var held) ? MapColors((Color[])held.Clone(), FromShader) : null;

    internal void GetColorArray(int id, List<Color> destination) => Fill(GetColorArray(id), destination);

    private static Color[] MapColors(Color[] colors, Func<Color, Color> convert)
    {
        for (var index = 0; index < colors.Length; index++) colors[index] = convert(colors[index]);
        return colors;
    }
}
