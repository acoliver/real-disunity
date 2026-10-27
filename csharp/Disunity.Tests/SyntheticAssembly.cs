using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using System.Reflection.PortableExecutable;

/// <summary>
/// Writes small assemblies with System.Reflection.Metadata, so the player tests need no
/// Unity or game binaries: modules that define types, and consumers that reference them.
/// Types are public classes deriving from System.Object with public instance fields.
/// </summary>
internal sealed class SyntheticAssembly
{
    private readonly MetadataBuilder _metadata = new();
    private readonly TypeReferenceHandle _object;
    private int _fields = 1;

    public SyntheticAssembly(string name)
    {
        _metadata.AddModule(0, _metadata.GetOrAddString(name + ".dll"), _metadata.GetOrAddGuid(Guid.NewGuid()), default, default);
        _metadata.AddAssembly(_metadata.GetOrAddString(name), new Version(1, 0, 0, 0), default, default, default, AssemblyHashAlgorithm.None);
        var runtime = Reference("System.Runtime", new Version(8, 0, 0, 0), new byte[] { 0xb0, 0x3f, 0x5f, 0x7f, 0x11, 0xd5, 0x0a, 0x3a });
        _object = TypeRef(runtime, "System", "Object");
        _metadata.AddTypeDefinition(default, default, _metadata.GetOrAddString("<Module>"), default,
            MetadataTokens.FieldDefinitionHandle(1), MetadataTokens.MethodDefinitionHandle(1));
    }

    public AssemblyReferenceHandle Reference(string name, Version? version = null, byte[]? token = null) =>
        _metadata.AddAssemblyReference(_metadata.GetOrAddString(name), version ?? new Version(0, 0, 0, 0), default,
            token is null ? default : _metadata.GetOrAddBlob(token), default, default);

    public TypeReferenceHandle TypeRef(AssemblyReferenceHandle scope, string ns, string name) =>
        _metadata.AddTypeReference(scope, _metadata.GetOrAddString(ns), _metadata.GetOrAddString(name));

    /// <summary>Defines a public class with public instance fields of the given class types.</summary>
    public void DefineClass(string ns, string name, params (string Field, EntityHandle Type)[] fields)
    {
        _metadata.AddTypeDefinition(
            TypeAttributes.Public | TypeAttributes.Class,
            _metadata.GetOrAddString(ns),
            _metadata.GetOrAddString(name),
            _object,
            MetadataTokens.FieldDefinitionHandle(_fields),
            MetadataTokens.MethodDefinitionHandle(1));
        foreach (var (field, type) in fields)
        {
            var signature = new BlobBuilder();
            new BlobEncoder(signature).FieldSignature().Type(type, isValueType: false);
            _metadata.AddFieldDefinition(FieldAttributes.Public, _metadata.GetOrAddString(field), _metadata.GetOrAddBlob(signature));
            _fields++;
        }
    }

    /// <summary>References an instance method returning void with <paramref name="parameters"/> int parameters.</summary>
    public void MethodRef(TypeReferenceHandle parent, string name, int parameters)
    {
        var signature = new BlobBuilder();
        new BlobEncoder(signature).MethodSignature(isInstanceMethod: true).Parameters(parameters,
            returnType => returnType.Void(),
            list => { for (var i = 0; i < parameters; i++) list.AddParameter().Type().Int32(); });
        _metadata.AddMemberReference(parent, _metadata.GetOrAddString(name), _metadata.GetOrAddBlob(signature));
    }

    /// <summary>References an int field.</summary>
    public void FieldRef(TypeReferenceHandle parent, string name)
    {
        var signature = new BlobBuilder();
        new BlobEncoder(signature).FieldSignature().Int32();
        _metadata.AddMemberReference(parent, _metadata.GetOrAddString(name), _metadata.GetOrAddBlob(signature));
    }

    public byte[] Build()
    {
        var image = new BlobBuilder();
        new ManagedPEBuilder(PEHeaderBuilder.CreateLibraryHeader(), new MetadataRootBuilder(_metadata), new BlobBuilder())
            .Serialize(image);
        return image.ToArray();
    }
}
