namespace Disunity.Hosting;

/// <summary>Who made a shader declaration. A player's plan knows the shader; the engine declares only the shaders it defines
/// itself, and what it declares gives way to the plan's declaration of the same name.</summary>
internal enum DeclarationOrigin
{
    Plan,
    Engine,
}

/// <summary>What is declared about a shader: the render queue, the number of passes in the first sub-shader and their names in
/// order when known, the number of sub-shaders and, when the declaration carries them, the first sub-shader's tags in declared
/// order and the name of the fallback shader (the empty string for none). A count, the names, the tags or the fallback left
/// out are unknown.</summary>
internal sealed record ShaderDeclaration(int RenderQueue, int? PassCount, int? SubshaderCount, IReadOnlyList<string>? PassNames = null,
    IReadOnlyList<KeyValuePair<string, string>>? Tags = null, string? Fallback = null, DeclarationOrigin Origin = DeclarationOrigin.Plan);
