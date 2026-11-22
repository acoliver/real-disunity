namespace Disunity.Hosting;

/// <summary>What a player's plan declares about a shader: the render queue, the names of the first sub-shader's passes in order
/// (their number is the pass count), the number of sub-shaders and, when the plan carries them, the first sub-shader's tags in
/// declared order and the name of the fallback shader (the empty string for none). A shader declared without tags or fallback
/// leaves them unknown.</summary>
internal sealed record ShaderDeclaration(int RenderQueue, IReadOnlyList<string> PassNames, int SubshaderCount,
    IReadOnlyList<KeyValuePair<string, string>>? Tags = null, string? Fallback = null);
