using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using System.Reflection.PortableExecutable;

namespace Disunity.Hosting;

/// <summary>
/// A Unity engine module of a built player, for example <c>UnityEngine.CoreModule</c>:
/// its identity and the full names of the top-level public types it defines or forwards.
/// </summary>
public sealed record UnityModule(AssemblyName Name, IReadOnlyList<string> Types);

/// <summary>
/// Facades that stand in for a player's Unity engine modules. Unity's module assemblies call
/// into Unity's native player, so they are never loaded. Game code references types through
/// them, for example <c>[UnityEngine.CoreModule]UnityEngine.Vector3</c>, and .NET requires an
/// assembly with the requested name. A facade has the module's identity and forwards each
/// type that Disunity.Engine defines; types it does not define stay unresolved, so code that
/// uses one fails with a <see cref="TypeLoadException"/> naming the type.
/// </summary>
public static class ModuleFacades
{
    /// <summary>
    /// Whether an assembly name is a Unity engine module: <c>UnityEngine</c> or
    /// <c>UnityEngine.*Module</c>. Package assemblies such as <c>UnityEngine.UI</c> have a
    /// managed implementation and load like game code.
    /// </summary>
    public static bool IsEngineModule(string name) =>
        name == "UnityEngine"
        || (name.StartsWith("UnityEngine.", StringComparison.Ordinal)
            && name.EndsWith("Module", StringComparison.Ordinal));

    /// <summary>Reads a module's identity and type names from its metadata.</summary>
    public static UnityModule Read(Stream module)
    {
        using var pe = new PEReader(module, PEStreamOptions.LeaveOpen);
        var reader = pe.GetMetadataReader();
        var types = new List<string>();
        foreach (var handle in reader.TypeDefinitions)
        {
            var type = reader.GetTypeDefinition(handle);
            var visibility = type.Attributes & TypeAttributes.VisibilityMask;
            if (visibility == TypeAttributes.Public)
                types.Add(FullName(reader, type.Namespace, type.Name));
        }
        foreach (var handle in reader.ExportedTypes)
        {
            var exported = reader.GetExportedType(handle);
            if (exported.Implementation.Kind == HandleKind.AssemblyReference)
                types.Add(FullName(reader, exported.Namespace, exported.Name));
        }
        return new UnityModule(reader.GetAssemblyDefinition().GetAssemblyName(), types);
    }

    /// <summary>The module's types that <paramref name="target"/> defines as public top-level types.</summary>
    public static IReadOnlyList<string> Implemented(UnityModule module, Assembly target) =>
        module.Types.Where(name => target.GetType(name) is { IsPublic: true }).ToList();

    /// <summary>
    /// Builds the facade for a module: an assembly with the module's name and version whose
    /// only content is a forwarder to <paramref name="target"/> for each implemented type.
    /// </summary>
    public static byte[] Build(UnityModule module, Assembly target)
    {
        var metadata = new MetadataBuilder();
        var name = module.Name.Name ?? throw new ArgumentException("module has no name", nameof(module));
        metadata.AddModule(0, metadata.GetOrAddString(name + ".dll"), metadata.GetOrAddGuid(Guid.NewGuid()), default, default);
        metadata.AddAssembly(
            metadata.GetOrAddString(name),
            module.Name.Version ?? new Version(0, 0, 0, 0),
            default,
            PublicKey(metadata, module.Name),
            default,
            AssemblyHashAlgorithm.None);
        var targetName = target.GetName();
        var reference = metadata.AddAssemblyReference(
            metadata.GetOrAddString(targetName.Name!),
            targetName.Version ?? new Version(0, 0, 0, 0),
            default,
            default,
            default,
            default);
        // Row 1 of the type table is the <Module> type every assembly has.
        metadata.AddTypeDefinition(
            default, default, metadata.GetOrAddString("<Module>"), default,
            MetadataTokens.FieldDefinitionHandle(1), MetadataTokens.MethodDefinitionHandle(1));
        foreach (var type in Implemented(module, target))
        {
            var split = type.LastIndexOf('.');
            var ns = split < 0 ? "" : type[..split];
            metadata.AddExportedType(
                Forwarder,
                metadata.GetOrAddString(ns),
                metadata.GetOrAddString(type[(split + 1)..]),
                reference,
                0);
        }
        var image = new BlobBuilder();
        new ManagedPEBuilder(
            PEHeaderBuilder.CreateLibraryHeader(),
            new MetadataRootBuilder(metadata),
            new BlobBuilder()).Serialize(image);
        return image.ToArray();
    }

    /// <summary>The flag that marks an exported type as a type forwarder (ECMA-335 II.23.1.15).</summary>
    private const TypeAttributes Forwarder = (TypeAttributes)0x00200000;

    private static BlobHandle PublicKey(MetadataBuilder metadata, AssemblyName name) =>
        name.GetPublicKey() is { Length: > 0 } key ? metadata.GetOrAddBlob(key) : default;

    private static string FullName(MetadataReader reader, StringHandle ns, StringHandle name)
    {
        var space = reader.GetString(ns);
        return space.Length == 0 ? reader.GetString(name) : $"{space}.{reader.GetString(name)}";
    }
}
