using Disunity.Hosting;

namespace UnityEngine;

/// <summary>The tags a shader declares. They are the tags of its first sub-shader, which is the sub-shader whose passes the
/// engine names; the tags of single passes are not kept. A tag name matches in any case, because a player stores the names
/// of some built-in tags in upper case (<c>QUEUE</c>, <c>IGNOREPROJECTOR</c>, <c>LIGHTMODE</c>) and keeps the case of the others.</summary>
public sealed partial class Shader
{
    private Dictionary<string, string>? _tags;
    private string? _fallback;

    private static Dictionary<string, string> TagTable(IEnumerable<KeyValuePair<string, string>> tags)
    {
        var table = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var (tag, value) in tags) table.TryAdd(tag, value);
        return table;
    }

    /// <summary>Records the tags and the fallback a plan declares, keeping what an earlier declaration knew when this one
    /// leaves it out.</summary>
    internal void DeclareTags(ShaderDeclaration declaration)
    {
        if (declaration.Tags is not null) _tags = TagTable(declaration.Tags);
        _fallback = declaration.Fallback ?? _fallback;
    }

    /// <summary>Whether the declaration's tags and fallback can stand beside the known ones: each that both know is the same.</summary>
    internal bool TagsAgree(ShaderDeclaration declaration)
    {
        var sameTags = _tags is null || declaration.Tags is null || SameTags(_tags, TagTable(declaration.Tags));
        return sameTags && (_fallback is null || declaration.Fallback is null || _fallback == declaration.Fallback);
    }

    private static bool SameTags(Dictionary<string, string> known, Dictionary<string, string> other) =>
        known.Count == other.Count && known.All(tag => other.TryGetValue(tag.Key, out var value) && value == tag.Value);

    /// <summary>The tags as declared, for reporting a disagreement.</summary>
    internal string DescribeTags() => FormatTags(_tags);

    /// <summary>Tags as <c>name=value</c> pairs in order, or <c>unknown</c> for tags nothing declared.</summary>
    internal static string FormatTags(IEnumerable<KeyValuePair<string, string>>? tags) =>
        tags is null ? "unknown" : string.Join(", ", tags.Select(tag => $"{tag.Key}={tag.Value}"));

    /// <summary>The value of <paramref name="tag"/> among the shader's tags, or among those of its fallback shader and that
    /// shader's fallback in turn when <paramref name="searchFallbacks"/> is set, or null when none declares it. A shader whose
    /// tags are not declared throws <see cref="NotSupportedException"/> naming <paramref name="member"/>, and so does a search
    /// that needs a fallback the engine does not know: one whose declaration is unknown or whose shader the engine does not have.</summary>
    internal string? FindTag(string tag, bool searchFallbacks, string member)
    {
        var visited = new HashSet<Shader>(ReferenceEqualityComparer.Instance);
        for (var shader = this; shader is not null && visited.Add(shader); shader = shader.FallbackShader(member))
        {
            if (shader.DeclaredTags(member).TryGetValue(tag, out var value)) return value;
            if (!searchFallbacks) break;
        }
        return null;
    }

    private Dictionary<string, string> DeclaredTags(string member) =>
        _tags ?? throw new NotSupportedException($"{member} is not supported yet: the engine has no tags for shader '{name}'");

    private Shader? FallbackShader(string member)
    {
        var fallback = _fallback ?? throw new NotSupportedException($"{member} cannot search fallbacks: the engine has no fallback declaration for shader '{name}'");
        if (fallback.Length == 0) return null;
        return DisunityHost.FindShader(fallback)
            ?? throw new NotSupportedException($"{member} cannot search fallbacks: shader '{name}' falls back to '{fallback}', which the engine does not have");
    }
}
