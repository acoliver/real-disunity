using Disunity.Hosting;

namespace UnityEngine;

/// <summary>What the engine declares for the shaders it defines itself, which have no declarations to read: the sprite and UI
/// shaders. The queue is Transparent (3000) for all of them. The engine declares no tags for them and no fallback shader, so
/// <see cref="Material.GetTag(string, bool, string)"/> gives the caller's default. A player's plan that declares a shader of the
/// same name replaces these.</summary>
internal static class BuiltInShaders
{
    private const int Transparent = 3000;

    /// <summary>Sprites/Default is one subshader of one pass.</summary>
    internal static readonly ShaderDeclaration SpriteUnlit = Defined(passCount: 1, subshaderCount: 1);

    /// <summary>The URP lit sprite shader's pass and subshader counts are not known here.</summary>
    internal static readonly ShaderDeclaration SpriteLit = Defined(passCount: null, subshaderCount: null);

    /// <summary>UI/Default is one subshader of one pass.</summary>
    internal static readonly ShaderDeclaration CanvasDefault = Defined(passCount: 1, subshaderCount: 1);

    private static ShaderDeclaration Defined(int? passCount, int? subshaderCount) =>
        new(Transparent, passCount, subshaderCount, Tags: [], Fallback: "", Origin: DeclarationOrigin.Engine);
}
