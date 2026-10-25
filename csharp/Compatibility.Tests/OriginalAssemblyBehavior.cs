using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Text.Json;

internal static class OriginalAssemblyBehavior
{
    public static void Verify(string repository)
    {
        using var profiles = JsonDocument.Parse(File.ReadAllText(Path.Combine(repository,
            "csharp/Compatibility.Tests/OriginalGames/profiles.json")));
        foreach (var name in new[] { "Snake", "Breakout", "Pong" }) VerifyGame(name, profiles.RootElement);
    }

    private static void VerifyGame(string name, JsonElement profiles)
    {
        var source = profiles.GetProperty("sources").GetProperty(name);
        var inputs = Path.Combine(TestArtifacts.ManagedRoot, "artifacts/obj", name, "debug/original-sources.txt");
        var sources = File.ReadAllLines(inputs).Where(path => path.Contains("/Assets/", StringComparison.Ordinal)).ToArray();
        BinderBehavior.Check(sources.Length > 0, name + " compiler input set is empty");
        var root = sources[0][..sources[0].IndexOf("/Assets/", StringComparison.Ordinal)];
        var version = File.ReadLines(Path.Combine(root, "ProjectSettings/ProjectVersion.txt"))
            .Single(line => line.StartsWith("m_EditorVersion:", StringComparison.Ordinal)).Split(':', 2)[1].Trim();
        BinderBehavior.Check(version == source.GetProperty("observed_editor_version").GetString(), name + " editor version drift");
        using var stream = File.OpenRead(Path.Combine(TestArtifacts.ManagedRoot, "artifacts/bin", name, "debug", name + ".dll"));
        using var pe = new PEReader(stream);
        var reader = pe.GetMetadataReader();
        var types = reader.TypeDefinitions.Select(handle => reader.GetString(reader.GetTypeDefinition(handle).Name)).ToHashSet(StringComparer.Ordinal);
        foreach (var path in sources)
            BinderBehavior.Check(types.Contains(Path.GetFileNameWithoutExtension(path)), name + " PE missing source type " + path);
        var members = reader.MemberReferences.Select(reader.GetMemberReference)
            .Where(member => member.Parent.Kind == HandleKind.TypeReference)
            .Select(member => reader.GetTypeReference((TypeReferenceHandle)member.Parent))
            .Count(type => reader.GetString(type.Namespace).StartsWith("UnityEngine", StringComparison.Ordinal));
        BinderBehavior.Check(members > 0, name + " PE must reference facade API members");
        Console.WriteLine($"PASS original compiler inputs {name}: {sources.Length} source types, {members} facade references");
    }
}
