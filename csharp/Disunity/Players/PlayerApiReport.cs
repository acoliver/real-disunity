using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;

namespace Disunity.Hosting;

/// <summary>
/// A Unity engine type or member that a player's assemblies reference and Disunity.Engine
/// does not provide. <see cref="Member"/> is null for a missing type. <see cref="ReferencedBy"/>
/// names the assemblies that reference it.
/// </summary>
public sealed record MissingApi(string Module, string Type, string? Member, IReadOnlyList<string> ReferencedBy);

/// <summary>
/// Lists the engine API a player's assemblies reference that Disunity.Engine lacks, from
/// their metadata alone. Members match by name, kind (field or method) and parameter count;
/// overloads that differ only in parameter types are not told apart.
/// </summary>
public static class PlayerApiReport
{
    /// <summary>Reports the missing API referenced by the given assemblies.</summary>
    public static IReadOnlyList<MissingApi> Build(IEnumerable<(string Name, Stream Image)> assemblies, Assembly target)
    {
        var missing = new SortedDictionary<(string Module, string Type, string Member), SortedSet<string>>(Ordinal);
        foreach (var (name, image) in assemblies)
        {
            using var pe = new PEReader(image, PEStreamOptions.LeaveOpen);
            var reader = pe.GetMetadataReader();
            foreach (var key in MissingIn(reader, target))
            {
                if (!missing.TryGetValue(key, out var users)) missing[key] = users = new SortedSet<string>(StringComparer.Ordinal);
                users.Add(name);
            }
        }
        return missing
            .Select(entry => new MissingApi(entry.Key.Module, entry.Key.Type, entry.Key.Member.Length == 0 ? null : entry.Key.Member, entry.Value.ToList()))
            .ToList();
    }

    private static IEnumerable<(string, string, string)> MissingIn(MetadataReader reader, Assembly target)
    {
        foreach (var handle in reader.TypeReferences)
        {
            if (EngineType(reader, handle) is ({ } module, { } type) && target.GetType(type) is null)
                yield return (module, type, "");
        }
        foreach (var handle in reader.MemberReferences)
        {
            var member = reader.GetMemberReference(handle);
            if (member.Parent.Kind != HandleKind.TypeReference) continue;
            if (EngineType(reader, (TypeReferenceHandle)member.Parent) is not ({ } module, { } type)) continue;
            if (target.GetType(type) is not { } found) continue;
            var name = reader.GetString(member.Name);
            if (!HasMember(found, name, member, reader)) yield return (module, type, Describe(name, member, reader));
        }
    }

    /// <summary>The engine module and full name of a type reference, or nulls when it is not an engine type.</summary>
    private static (string? Module, string? Type) EngineType(MetadataReader reader, TypeReferenceHandle handle)
    {
        var reference = reader.GetTypeReference(handle);
        var name = reader.GetString(reference.Name);
        var scope = reference.ResolutionScope;
        if (scope.Kind == HandleKind.TypeReference)
        {
            // A nested type: Outer+Inner, the form reflection uses.
            var (module, outer) = EngineType(reader, (TypeReferenceHandle)scope);
            return (module, outer is null ? null : $"{outer}+{name}");
        }
        if (scope.Kind != HandleKind.AssemblyReference) return (null, null);
        var assembly = reader.GetString(reader.GetAssemblyReference((AssemblyReferenceHandle)scope).Name);
        if (!ModuleFacades.IsEngineModule(assembly)) return (null, null);
        var ns = reader.GetString(reference.Namespace);
        return (assembly, ns.Length == 0 ? name : $"{ns}.{name}");
    }

    /// <summary>Orders entries by module, type, then member, comparing text ordinally.</summary>
    private static readonly Comparer<(string Module, string Type, string Member)> Ordinal =
        Comparer<(string Module, string Type, string Member)>.Create((a, b) =>
        {
            var order = string.CompareOrdinal(a.Module, b.Module);
            if (order == 0) order = string.CompareOrdinal(a.Type, b.Type);
            return order != 0 ? order : string.CompareOrdinal(a.Member, b.Member);
        });

    private const BindingFlags Everything =
        BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static;

    private static bool HasMember(Type type, string name, MemberReference member, MetadataReader reader)
    {
        if (member.GetKind() == MemberReferenceKind.Field)
            return type.GetField(name, Everything) is not null;
        var count = ParameterCount(reader, member);
        if (name == ".ctor")
            return type.GetConstructors(Everything).Any(c => c.GetParameters().Length == count);
        return type.GetMethods(Everything).Any(m => m.Name == name && m.GetParameters().Length == count);
    }

    private static string Describe(string name, MemberReference member, MetadataReader reader) =>
        member.GetKind() == MemberReferenceKind.Field ? name : $"{name}({ParameterCount(reader, member)} parameters)";

    /// <summary>The parameter count from a method signature blob (ECMA-335 II.23.2.1).</summary>
    private static int ParameterCount(MetadataReader reader, MemberReference member)
    {
        var blob = reader.GetBlobReader(member.Signature);
        var header = blob.ReadSignatureHeader();
        if (header.IsGeneric) blob.ReadCompressedInteger();
        return blob.ReadCompressedInteger();
    }
}
