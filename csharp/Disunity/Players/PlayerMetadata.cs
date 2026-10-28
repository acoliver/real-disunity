using System.Collections.Immutable;
using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;

namespace Disunity.Hosting;

/// <summary>A type as a field or base-class signature names it, resolved against a player's assemblies.</summary>
internal abstract record SigType;

/// <summary>A built-in .NET type: a number, <c>bool</c>, <c>char</c>, <c>string</c>, <c>object</c> and so on.</summary>
internal sealed record PrimitiveSig(PrimitiveTypeCode Code) : SigType;

/// <summary>A type defined in one of the assemblies, with its generic arguments, if any.</summary>
internal sealed record DefinedSig(DefinedType Type, ImmutableArray<SigType> Args) : SigType
{
    public bool Equals(DefinedSig? other) =>
        other is not null && Type == other.Type && Args.SequenceEqual(other.Args);

    public override int GetHashCode() => Args.Aggregate(Type.GetHashCode(), HashCode.Combine);
}

/// <summary>A single-dimensional, zero-based array.</summary>
internal sealed record ArraySig(SigType Element) : SigType;

/// <summary>A type Unity never serializes: a pointer, a multi-dimensional array or a generic
/// method parameter.</summary>
internal sealed record UnsupportedSig(string What) : SigType;

/// <summary>A reference to a type none of the assemblies defines.</summary>
internal sealed record UnresolvedSig(string Name) : SigType;

/// <summary>A type definition in one assembly's metadata.</summary>
internal readonly record struct DefinedType(PlayerMetadata.Module Module, TypeDefinitionHandle Handle)
{
    public MetadataReader Reader => Module.Reader;
    public TypeDefinition Definition => Reader.GetTypeDefinition(Handle);
    public string Assembly => Module.Name;

    /// <summary>The full name, with nested types joined by <c>/</c> as in metadata.</summary>
    public string FullName
    {
        get
        {
            var definition = Definition;
            var name = Reader.GetString(definition.Name);
            if (definition.GetDeclaringType() is { IsNil: false } outer)
                return $"{new DefinedType(Module, outer).FullName}/{name}";
            var space = Reader.GetString(definition.Namespace);
            return space.Length == 0 ? name : $"{space}.{name}";
        }
    }
}

/// <summary>
/// The metadata of a set of assemblies, for resolving the types their signatures name.
/// Nothing is loaded for execution. Type forwarders are followed, so a reference through
/// <c>netstandard</c> finds the type where the player defines it.
/// </summary>
internal sealed class PlayerMetadata : IDisposable
{
    internal sealed record Module(string Name, MetadataReader Reader);

    private readonly List<PEReader> _images = new();
    private readonly Dictionary<string, Module> _modules = new(StringComparer.Ordinal);
    private readonly Dictionary<(string Assembly, string FullName), DefinedType?> _cache = new();

    public PlayerMetadata(IEnumerable<(string Name, byte[] Image)> assemblies)
    {
        foreach (var (_, image) in assemblies)
        {
            var pe = new PEReader(ImmutableArray.Create(image));
            if (!pe.HasMetadata) { pe.Dispose(); continue; }
            _images.Add(pe);
            var reader = pe.GetMetadataReader();
            if (!reader.IsAssembly) continue;
            var name = reader.GetString(reader.GetAssemblyDefinition().Name);
            _modules.TryAdd(name, new Module(name, reader));
        }
    }

    public IEnumerable<Module> Modules => _modules.Values;

    public void Dispose() => _images.ForEach(image => image.Dispose());

    /// <summary>The type with this full name (nested types after <c>/</c>) in the named assembly.</summary>
    public DefinedType? Find(string assembly, string fullName)
    {
        if (_cache.TryGetValue((assembly, fullName), out var known)) return known;
        // Not found until the search ends, so assemblies that forward a name to each other
        // end the search instead of recursing.
        _cache[(assembly, fullName)] = null;
        DefinedType? found = null;
        if (_modules.TryGetValue(assembly, out var module))
            found = Defined(module, fullName) ?? Forwarded(module, fullName);
        _cache[(assembly, fullName)] = found;
        return found;
    }

    private readonly Dictionary<string, Dictionary<string, DefinedType>> _definitions = new(StringComparer.Ordinal);

    private DefinedType? Defined(Module module, string fullName)
    {
        if (!_definitions.TryGetValue(module.Name, out var index))
        {
            index = new Dictionary<string, DefinedType>(StringComparer.Ordinal);
            foreach (var handle in module.Reader.TypeDefinitions)
            {
                var type = new DefinedType(module, handle);
                index.TryAdd(type.FullName, type);
            }
            _definitions[module.Name] = index;
        }
        return index.TryGetValue(fullName, out var found) ? found : null;
    }

    private DefinedType? Forwarded(Module module, string fullName)
    {
        var reader = module.Reader;
        var top = fullName.Split('/')[0];
        foreach (var handle in reader.ExportedTypes)
        {
            var exported = reader.GetExportedType(handle);
            if (exported.Implementation.Kind != HandleKind.AssemblyReference) continue;
            var space = reader.GetString(exported.Namespace);
            var name = reader.GetString(exported.Name);
            if ((space.Length == 0 ? name : $"{space}.{name}") != top) continue;
            var target = reader.GetAssemblyReference((AssemblyReferenceHandle)exported.Implementation);
            return Find(reader.GetString(target.Name), fullName);
        }
        return null;
    }

    /// <summary>Resolves a handle from <paramref name="module"/>: a definition, a reference or a
    /// specification (a generic instantiation or array), within a generic context.</summary>
    public SigType Resolve(Module module, EntityHandle handle, ImmutableArray<SigType> context)
    {
        var provider = new SigProvider(this, module);
        return handle.Kind switch
        {
            HandleKind.TypeDefinition => new DefinedSig(new DefinedType(module, (TypeDefinitionHandle)handle), ImmutableArray<SigType>.Empty),
            HandleKind.TypeReference => provider.GetTypeFromReference(module.Reader, (TypeReferenceHandle)handle, 0),
            HandleKind.TypeSpecification => module.Reader.GetTypeSpecification((TypeSpecificationHandle)handle).DecodeSignature(provider, context),
            _ => new UnsupportedSig($"a {handle.Kind} handle"),
        };
    }

    /// <summary>The type a field declares, within the generic context of its declaring type.</summary>
    public SigType FieldType(Module module, FieldDefinition field, ImmutableArray<SigType> context) =>
        field.DecodeSignature(new SigProvider(this, module), context);

    /// <summary>The full name a type reference names, and the assembly it names it in.</summary>
    internal (string Assembly, string FullName)? Target(MetadataReader reader, TypeReferenceHandle handle)
    {
        var reference = reader.GetTypeReference(handle);
        var name = reader.GetString(reference.Name);
        var scope = reference.ResolutionScope;
        switch (scope.Kind)
        {
            case HandleKind.TypeReference:
                return Target(reader, (TypeReferenceHandle)scope) is { } outer ? (outer.Assembly, $"{outer.FullName}/{name}") : null;
            case HandleKind.AssemblyReference:
                var assembly = reader.GetString(reader.GetAssemblyReference((AssemblyReferenceHandle)scope).Name);
                var space = reader.GetString(reference.Namespace);
                return (assembly, space.Length == 0 ? name : $"{space}.{name}");
            case HandleKind.ModuleDefinition:
                var own = reader.GetString(reader.GetAssemblyDefinition().Name);
                var ns = reader.GetString(reference.Namespace);
                return (own, ns.Length == 0 ? name : $"{ns}.{name}");
            default:
                return null;
        }
    }

    /// <summary>Decodes signatures into <see cref="SigType"/>s; the generic context is the type
    /// arguments of the type whose members are being read.</summary>
    private sealed class SigProvider(PlayerMetadata metadata, Module module) : ISignatureTypeProvider<SigType, ImmutableArray<SigType>>
    {
        public SigType GetPrimitiveType(PrimitiveTypeCode typeCode) => new PrimitiveSig(typeCode);

        public SigType GetTypeFromDefinition(MetadataReader reader, TypeDefinitionHandle handle, byte rawTypeKind) =>
            new DefinedSig(new DefinedType(module, handle), ImmutableArray<SigType>.Empty);

        public SigType GetTypeFromReference(MetadataReader reader, TypeReferenceHandle handle, byte rawTypeKind)
        {
            if (metadata.Target(reader, handle) is not { } target) return new UnsupportedSig("a reference outside any assembly");
            return metadata.Find(target.Assembly, target.FullName) is { } found
                ? new DefinedSig(found, ImmutableArray<SigType>.Empty)
                : new UnresolvedSig($"[{target.Assembly}]{target.FullName}");
        }

        public SigType GetTypeFromSpecification(MetadataReader reader, ImmutableArray<SigType> context, TypeSpecificationHandle handle, byte rawTypeKind) =>
            reader.GetTypeSpecification(handle).DecodeSignature(this, context);

        public SigType GetGenericInstantiation(SigType genericType, ImmutableArray<SigType> typeArguments) =>
            genericType is DefinedSig defined ? defined with { Args = typeArguments } : genericType;

        public SigType GetGenericTypeParameter(ImmutableArray<SigType> genericContext, int index) =>
            index < genericContext.Length ? genericContext[index] : new UnsupportedSig($"generic parameter {index}");

        public SigType GetSZArrayType(SigType elementType) => new ArraySig(elementType);
        public SigType GetArrayType(SigType elementType, ArrayShape shape) => new UnsupportedSig("a multi-dimensional array");
        public SigType GetByReferenceType(SigType elementType) => new UnsupportedSig("a by-reference type");
        public SigType GetPointerType(SigType elementType) => new UnsupportedSig("a pointer");
        public SigType GetFunctionPointerType(MethodSignature<SigType> signature) => new UnsupportedSig("a function pointer");
        public SigType GetGenericMethodParameter(ImmutableArray<SigType> genericContext, int index) => new UnsupportedSig("a generic method parameter");
        public SigType GetModifiedType(SigType modifier, SigType unmodifiedType, bool isRequired) => unmodifiedType;
        public SigType GetPinnedType(SigType elementType) => elementType;
    }
}
