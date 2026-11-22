namespace UnityEngine;

/// <summary>
/// The mesh renderers the window draws without a texture, grouped by the shader of their first material, with the texture
/// properties that material does hold. The window draws slot 0 with the main texture of its world material when that is a
/// <see cref="Texture2D"/> with pixels (see <see cref="Renderer.DrawableMainTexture"/>); a renderer whose material has no such
/// texture draws white, and the report says which shaders those are and where their textures are instead. <see cref="Survey"/>
/// reads the live renderers of the loaded world when it is called, so it reflects later material changes, and the play bridge
/// prints it.
/// </summary>
internal static class WhiteRenderers
{
    /// <summary>The shaders listed in a summary; the rest are counted.</summary>
    internal const int ShadersListed = 12;

    /// <summary>The renderers drawn without a texture whose material has <paramref name="Shader"/>, the texture properties
    /// their materials hold as "property:kind" in name order, and what their main textures are: "null", "empty" (a
    /// <see cref="Texture2D"/> of size 0) or "other", with several joined by "/".</summary>
    internal readonly record struct Group(string Shader, int Count, IReadOnlyList<string> Properties, string MainTexture);

    /// <summary>What <paramref name="Examined"/> mesh renderers drew: <paramref name="Total"/> without a texture, of which
    /// <paramref name="NoMaterial"/> have no material and the rest are in <paramref name="Groups"/>, the largest first.</summary>
    internal sealed record Report(int Examined, int Total, int NoMaterial, IReadOnlyList<Group> Groups)
    {
        /// <summary>The groups as one line of "shader count (properties; mainTexture state)" items cut after
        /// <see cref="ShadersListed"/> with "and N more", or null when no shader is reported.</summary>
        internal string? Summary => Groups.Count == 0 ? null : string.Join(", ", Listed());

        /// <summary>The lines the end of a run prints, none when no mesh renderer was examined.</summary>
        internal IReadOnlyList<string> Lines
        {
            get
            {
                if (Examined == 0) return [];
                var totals = $"renderers drawn without a texture: {Total} of {Examined} mesh renderers, {NoMaterial} with no material";
                return Summary is { } summary ? [$"renderers drawn without a texture by shader: {summary}", totals] : [totals];
            }
        }

        private IEnumerable<string> Listed()
        {
            foreach (var group in Groups.Take(ShadersListed)) yield return Describe(group);
            if (Groups.Count > ShadersListed) yield return $"and {Groups.Count - ShadersListed} more";
        }

        private static string Describe(Group group)
        {
            var properties = group.Properties.Count == 0 ? "no texture properties" : string.Join(' ', group.Properties);
            return $"{group.Shader} {group.Count} ({properties}; mainTexture {group.MainTexture})";
        }
    }

    /// <summary>A renderer drawn without a texture: the shader of its material (null without one), the textures that material
    /// holds as "property:kind", and its main texture's state.</summary>
    private sealed record Finding(string? Shader, IReadOnlyList<string> Properties, string MainTexture);

    /// <summary>The live mesh renderers of the world, read now.</summary>
    internal static Report Survey() => Survey(ObjectRegistry.SceneObjects(typeof(MeshRenderer)).Cast<MeshRenderer>());

    internal static Report Survey(IEnumerable<MeshRenderer> renderers) =>
        Build(renderers.Select(renderer => Observe(renderer.ReadShared() is [var first, ..] ? first : null)).ToArray());

    /// <summary>The finding for a slot drawn without a texture, or null when the slot is drawn with one.</summary>
    private static Finding? Observe(Material? first)
    {
        if (first is not { } material || !material) return new Finding(null, [], "null");
        if (Renderer.DrawableMainTexture(material) is not null) return null;
        var held = material.HeldTextures().Select(texture => $"{texture.Property}:{KindOf(texture.Texture)}").ToArray();
        return new Finding(ShaderName(material), held, MainTextureState(material.mainTexture));
    }

    private static string ShaderName(Material material) =>
        material.shader is { } shader && shader ? (shader.name is { Length: > 0 } name ? name : "(unnamed)") : "(no shader)";

    private static string KindOf(Texture texture) => texture switch
    {
        RenderTexture => "RenderTexture",
        Texture2DArray => "Texture2DArray",
        Cubemap => "Cubemap",
        Texture3D => "Texture3D",
        Texture2D => "Texture2D",
        _ => "other",
    };

    private static string MainTextureState(Texture? texture) => texture switch
    {
        null => "null",
        Texture2D plain when plain && (plain.width <= 0 || plain.height <= 0) => "empty",
        _ => "other",
    };

    private static Report Build(Finding?[] findings)
    {
        var white = findings.OfType<Finding>().ToArray();
        var groups = white.Where(finding => finding.Shader is not null).GroupBy(finding => finding.Shader!, StringComparer.Ordinal)
            .Select(Merge).OrderByDescending(group => group.Count).ThenBy(group => group.Shader, StringComparer.Ordinal).ToArray();
        return new Report(findings.Length, white.Length, white.Length - groups.Sum(group => group.Count), groups);
    }

    private static Group Merge(IGrouping<string, Finding> findings) => new(
        findings.Key,
        findings.Count(),
        findings.SelectMany(finding => finding.Properties).Distinct().Order(StringComparer.Ordinal).ToArray(),
        string.Join('/', findings.Select(finding => finding.MainTexture).Distinct().Order(StringComparer.Ordinal)));
}
