namespace UnityEngine;

/// <summary>
/// The textures the engine could not send to the world, which therefore read there as no texture. A null texture is a game's own
/// "no texture" and is not noted. Every other texture sent as none is counted under its property name and the reason it has no
/// world form, so a run says why a global or material texture arrived empty. The play bridge prints the summary when the run ends.
/// The notes belong to the world loaded: a new world starts with none.
/// </summary>
internal static class UnsentTextures
{
    /// <summary>The number of times <paramref name="Property"/> was sent as none for <paramref name="Reason"/>.</summary>
    internal readonly record struct Entry(string Property, string Reason, int Count);

    private static readonly Dictionary<(string Property, string Reason), int> Counts = new();

    /// <summary>The counts so far, ordered by property name and then reason.</summary>
    internal static IReadOnlyList<Entry> Entries
    {
        get
        {
            lock (Counts)
            {
                return Counts.Select(c => new Entry(c.Key.Property, c.Key.Reason, c.Value))
                    .OrderBy(e => e.Property, StringComparer.Ordinal).ThenBy(e => e.Reason, StringComparer.Ordinal).ToArray();
            }
        }
    }

    /// <summary>The counts as one line of "property reason count" items, or null when nothing was noted.</summary>
    internal static string? Summary => Entries is { Count: > 0 } entries
        ? string.Join(", ", entries.Select(e => $"{e.Property} {e.Reason} {e.Count}"))
        : null;

    /// <summary>Counts a send of <paramref name="texture"/> as the texture of <paramref name="property"/> that arrived as none.
    /// A null texture is not noted.</summary>
    internal static void Note(string property, Texture? texture)
    {
        if (texture is null) return;
        var key = (property, ReasonOf(texture));
        lock (Counts) Counts[key] = Counts.GetValueOrDefault(key) + 1;
    }

    /// <summary>Why the world has no form for a texture: the texture was destroyed, a <see cref="Texture2D"/> has no pixels
    /// (with its size), an array, cube or volume the host cannot store (see <c>WorldGraphics.TextureOf</c>), or the kind of
    /// texture, by its type name, is one the world does not hold.</summary>
    private static string ReasonOf(Texture texture) => texture switch
    {
        RenderTexture => "destroyed-render-texture",
        Texture2D plain when !plain => "destroyed-texture2d",
        Texture2D plain => $"empty-texture2d:{plain.width}x{plain.height}",
        Texture2DArray or Cubemap or Texture3D => $"{(texture ? "unstorable" : "destroyed")}-{texture.GetType().Name.ToLowerInvariant()}",
        _ => $"unsupported-kind:{texture.GetType().Name}",
    };

    /// <summary>Forgets the notes, so the next world starts its own.</summary>
    internal static void Reset()
    {
        lock (Counts) Counts.Clear();
    }
}
