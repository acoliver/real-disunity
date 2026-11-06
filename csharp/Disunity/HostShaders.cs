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
    /// sub-shader (their number is its pass count) and the number of sub-shaders. A name that is registered already keeps
    /// its shader and its first declaration, which another reference to the same shader meets. A later declaration that
    /// disagrees with the first, in the queue, a count or a pass name, is reported to <paramref name="difference"/> and not
    /// applied; one that adds a count or the names the first left unknown completes it. The serialized file and object the
    /// shader's programs are in, when the plan names them, go to the world with the shader; the first source stays.
    /// </summary>
    internal static Shader DeclareShader(string name, int renderQueue, IReadOnlyList<string> passNames, int subshaderCount, Action<string> difference,
        string? file = null, long pathId = 0)
    {
        lock (Shaders)
        {
            var shader = RegisterShader(name);
            if (file is not null && shader.SourceFile is null) shader.SetSource(file, pathId);
            if (shader.Agrees(renderQueue, passNames, subshaderCount)) return shader.Declare(renderQueue, passNames, subshaderCount);
            difference($"shader {name}: declared again as queue {renderQueue}, {passNames.Count} passes ({string.Join(", ", passNames)}) and {subshaderCount} sub-shaders, which differs from its first declaration; the first stays");
            return shader;
        }
    }

    internal static Shader? FindShader(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        lock (Shaders) return Shaders.TryGetValue(name, out var shader) && shader ? shader : null;
    }
}
