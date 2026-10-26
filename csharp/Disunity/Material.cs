namespace UnityEngine;

public enum BuiltinSpriteMaterial : uint { Unlit = 0, Lit = 1 }

/// <summary>The built-in sprite materials supported by the native 2D pipeline.</summary>
public sealed class Material
{
    public BuiltinSpriteMaterial SpriteMaterial { get; }
    public static Material SpriteUnlit { get; } = new(BuiltinSpriteMaterial.Unlit);
    public static Material SpriteLit { get; } = new(BuiltinSpriteMaterial.Lit);
    public Material(BuiltinSpriteMaterial material)
    {
        if (material is not (BuiltinSpriteMaterial.Unlit or BuiltinSpriteMaterial.Lit))
            throw new NotSupportedException($"unsupported sprite material {material}");
        SpriteMaterial = material;
    }
    internal static Material FromNative(uint value) => value switch
    {
        0 => SpriteUnlit, 1 => SpriteLit,
        _ => throw new InvalidOperationException($"invalid native sprite material {value}")
    };
}
