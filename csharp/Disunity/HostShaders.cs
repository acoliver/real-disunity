using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static readonly Dictionary<string, Shader> Shaders = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>
    /// Makes <see cref="Shader.Find"/> return a shader of this name, which player loading calls for each
    /// shader the player has. Registering a name again returns the same shader. Names are matched
    /// without regard to case. The registry outlives worlds, like the other managed assets.
    /// </summary>
    public static Shader RegisterShader(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        lock (Shaders)
        {
            if (Shaders.TryGetValue(name, out var known) && known) return known;
            return Shaders[name] = new Shader(name);
        }
    }

    /// <summary>
    /// Registers a player's shader with what its plan declares: the render queue, the names of the passes in its first
    /// sub-shader (their number is its pass count), the number of sub-shaders and, when the plan carries them, the first
    /// sub-shader's tags and the fallback shader's name. A name that is registered already keeps
    /// its shader and its first declaration, which another reference to the same shader meets. A later declaration that
    /// disagrees with the first, in the queue, a count, a pass name, the tags or the fallback, is reported to <paramref name="difference"/> and not
    /// applied; one that adds a count, the names, the tags or the fallback the first left unknown completes it. A shader the engine defines
    /// itself (the sprite and UI shaders, see <see cref="BuiltInShaders"/>) is the exception: a plan's declaration of its name replaces the
    /// engine's without a difference, and the engine's declaration made after the plan's changes nothing. The serialized file and object the
    /// shader's programs are in, when the plan names them, go to the world with the shader; the first source stays.
    /// </summary>
    internal static Shader DeclareShader(string name, ShaderDeclaration declaration, Action<string> difference,
        string? file = null, long pathId = 0)
    {
        lock (Shaders)
        {
            var shader = RegisterShader(name);
            if (file is not null && shader.SourceFile is null) shader.SetSource(file, pathId);
            if (shader.Agrees(declaration)) return shader.Declare(declaration);
            difference($"shader {name}: declared again as queue {declaration.RenderQueue}, {declaration.PassCount?.ToString() ?? "unknown"} passes "
                + $"({string.Join(", ", declaration.PassNames ?? [])}), {declaration.SubshaderCount?.ToString() ?? "unknown"} sub-shaders, tags ({Shader.FormatTags(declaration.Tags)}) "
                + $"and fallback '{declaration.Fallback ?? "unknown"}', which differs from its first declaration (tags {shader.DescribeTags()}); the first stays");
            return shader;
        }
    }

    internal static Shader? FindShader(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        lock (Shaders) return Shaders.TryGetValue(name, out var shader) && shader ? shader : null;
    }
}
