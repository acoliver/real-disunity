using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Text.Json;

internal static class SourceAssemblyEvidence
{
    public static void Verify(string repository)
    {
        var output = Environment.GetEnvironmentVariable("DISUNITY_TEST_MANAGED_ROOT") ?? Path.Combine(repository, ".work/managed");
        using var census = JsonDocument.Parse(File.ReadAllText(Path.Combine(output, "source-census.json")));
        using var profiles = JsonDocument.Parse(File.ReadAllText(Path.Combine(repository, "csharp/Compatibility.Tests/OriginalGames/profiles.json")));
        var games = new Dictionary<string, object>();
        foreach (var name in new[] { "Snake", "Breakout", "Pong" })
        {
            var source = profiles.RootElement.GetProperty("sources").GetProperty(name);
            var versionPath = Path.Combine(repository, ".work/games", name.ToLowerInvariant(), "ProjectSettings/ProjectVersion.txt");
            var version = File.ReadLines(versionPath).Single(l => l.StartsWith("m_EditorVersion:", StringComparison.Ordinal)).Split(':', 2)[1].Trim();
            BinderBehavior.Check(version == source.GetProperty("observed_editor_version").GetString(), name + " source editor version drift");
            var assembly = Path.Combine(output, "artifacts/bin", name, "debug", name + ".dll");
            using var stream = File.OpenRead(assembly);
            using var pe = new PEReader(stream);
            var reader = pe.GetMetadataReader();
            var types = reader.TypeDefinitions.Select(h => reader.GetString(reader.GetTypeDefinition(h).Name)).ToHashSet(StringComparer.Ordinal);
            var sources = census.RootElement.GetProperty("sources").GetProperty(name).EnumerateObject().ToArray();
            foreach (var file in sources)
                BinderBehavior.Check(types.Contains(Path.GetFileNameWithoutExtension(file.Name)), name + " PE missing original source type " + file.Name);
            var members = new SortedSet<string>(StringComparer.Ordinal);
            foreach (var handle in reader.MemberReferences)
            {
                var member = reader.GetMemberReference(handle);
                if (member.Parent.Kind != HandleKind.TypeReference) continue;
                var type = reader.GetTypeReference((TypeReferenceHandle)member.Parent);
                var ns = reader.GetString(type.Namespace);
                if (!ns.StartsWith("UnityEngine", StringComparison.Ordinal)) continue;
                members.Add(ns + "." + reader.GetString(type.Name) + "." + reader.GetString(member.Name));
            }
            BinderBehavior.Check(members.Count > 0, name + " PE must reference actual facade API members");
            games.Add(name, new
            {
                source_editor_version = version, default_profile = source.GetProperty("default_profile").GetString(),
                compiler_runtime = "net8.0", assembly, source_count = sources.Length,
                source_types_verified = sources.Select(s => Path.GetFileNameWithoutExtension(s.Name)).ToArray(),
                emitted_types = types.Order(StringComparer.Ordinal).ToArray(), unity_api_member_references = members.ToArray(),
                field_aliases = source.GetProperty("field_aliases").Clone()
            });
            Console.WriteLine($"PASS source engineering evidence {name}: {sources.Length} original file types in PE, {members.Count} facade API member refs, observed source {version}");
        }
        File.WriteAllText(Path.Combine(output, "source-api-evidence.json"), JsonSerializer.Serialize(new
        {
            scope = profiles.RootElement.GetProperty("scope").GetString(),
            profiles = profiles.RootElement.GetProperty("profiles").Clone(), api_adapters = profiles.RootElement.GetProperty("api_adapters").Clone(), games
        }, new JsonSerializerOptions { WriteIndented = true }) + "\n");
    }
}
