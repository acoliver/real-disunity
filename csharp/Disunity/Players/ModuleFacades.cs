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

    /// <summary>
    /// Whether a facade forwards the type with this reflection name (nested types as
    /// <c>Outer+Inner</c>): <paramref name="target"/> defines it, and it and every type
    /// enclosing it are public.
    /// </summary>
    public static bool Forwards(Assembly target, string name) => target.GetType(name) is { IsVisible: true };

    /// <summary>The module's top-level types that a facade forwards to <paramref name="target"/>.</summary>
    public static IReadOnlyList<string> Implemented(UnityModule module, Assembly target) =>
        module.Types.Where(name => Forwards(target, name)).ToList();

    /// <summary>
    /// Builds the facade for a module: an assembly with the module's name and version whose
    /// only content is a forwarder to <paramref name="target"/> for each implemented type.
    /// </summary>
    public static byte[] Build(UnityModule module, Assembly target)
    {
        var metadata = new MetadataBuilder();
        var name = module.Name.Name ?? throw new ArgumentException("module has no name", nameof(module));
        metadata.AddModule(0, metadata.GetOrAddString(name + ".dll"), metadata.GetOrAddGuid(Guid.NewGuid()), default, default);
        // A load context checks only the simple name of an assembly it is given. Unity's
        // modules are unsigned, so the facade copies the version and no public key.
        metadata.AddAssembly(
            metadata.GetOrAddString(name),
            module.Name.Version ?? new Version(0, 0, 0, 0),
            default,
            default,
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
        foreach (var typeName in Implemented(module, target))
        {
            var type = target.GetType(typeName)!;
            var forwarder = metadata.AddExportedType(
                Forwarder,
                metadata.GetOrAddString(type.Namespace ?? ""),
                metadata.GetOrAddString(type.Name),
                reference,
                0);
            ForwardNested(metadata, forwarder, type);
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

    /// <summary>
    /// Forwards the public types nested in <paramref name="type"/>. The runtime resolves a
    /// nested type through rows naming the enclosing forwarder, with no flags and no
    /// namespace, as Unity's own <c>UnityEngine.dll</c> writes them.
    /// </summary>
    private static void ForwardNested(MetadataBuilder metadata, ExportedTypeHandle enclosing, Type type)
    {
        foreach (var nested in type.GetNestedTypes(BindingFlags.Public))
        {
            var row = metadata.AddExportedType(default, default, metadata.GetOrAddString(nested.Name), enclosing, 0);
            ForwardNested(metadata, row, nested);
        }
    }

    private static string FullName(MetadataReader reader, StringHandle ns, StringHandle name)
    {
        var space = reader.GetString(ns);
        return space.Length == 0 ? reader.GetString(name) : $"{space}.{reader.GetString(name)}";
    }
}
