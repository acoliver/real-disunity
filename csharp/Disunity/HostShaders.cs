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

    internal static Shader? FindShader(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        lock (Shaders) return Shaders.TryGetValue(name, out var shader) && shader ? shader : null;
    }
}
