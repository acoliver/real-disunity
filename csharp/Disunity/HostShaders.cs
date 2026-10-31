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
    /// Registers a player's shader with what its plan declares: the render queue and the numbers of passes in its first
    /// sub-shader and of sub-shaders. A name that is registered already keeps its shader and its first declaration,
    /// which another reference to the same shader meets. A later declaration that disagrees with the first is
    /// reported to <paramref name="difference"/> and not applied; one that adds a count the first left unknown
    /// completes it.
    /// </summary>
    internal static Shader DeclareShader(string name, int renderQueue, int passCount, int subshaderCount, Action<string> difference)
    {
        lock (Shaders)
        {
            var shader = RegisterShader(name);
            if (shader.Agrees(renderQueue, passCount, subshaderCount)) return shader.Declare(renderQueue, passCount, subshaderCount);
            difference($"shader {name}: declared again as queue {renderQueue}, {passCount} passes and {subshaderCount} sub-shaders, which differs from its first declaration; the first stays");
            return shader;
        }
    }

    internal static Shader? FindShader(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        lock (Shaders) return Shaders.TryGetValue(name, out var shader) && shader ? shader : null;
    }
}
