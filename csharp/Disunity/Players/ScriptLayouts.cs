using System.Collections.Immutable;
using System.Reflection;
using System.Reflection.Metadata;
using System.Text.Json;

namespace Disunity.Hosting;

/// <summary>How Unity serializes a script field's value.</summary>
public abstract record ScriptFieldType;

/// <summary>A number, <c>bool</c>, <c>char</c> or <c>string</c>, by Unity's type tree name:
/// <c>bool</c>, <c>SInt8</c>, <c>UInt8</c>, <c>SInt16</c>, <c>UInt16</c>, <c>char</c>,
/// <c>int</c>, <c>unsigned int</c>, <c>SInt64</c>, <c>UInt64</c>, <c>float</c>,
/// <c>double</c> or <c>string</c>. Enums are their underlying type.</summary>
public sealed record ScriptPrimitive(string Name) : ScriptFieldType;

/// <summary>A reference to a <c>UnityEngine.Object</c> of the named type: a file index and a path ID.</summary>
public sealed record ScriptObject(string Type) : ScriptFieldType;

/// <summary>A Unity engine value type with a native layout, such as <c>UnityEngine.Vector3</c>
/// or <c>UnityEngine.AnimationCurve</c>.</summary>
public sealed record ScriptBuiltin(string Type) : ScriptFieldType;

/// <summary>A <c>[Serializable]</c> class or struct, whose fields are listed under <see cref="Key"/>.</summary>
public sealed record ScriptComposite(string Key) : ScriptFieldType;

/// <summary>An array or <c>List&lt;T&gt;</c>.</summary>
public sealed record ScriptArray(ScriptFieldType Element) : ScriptFieldType;

/// <summary>A <c>[SerializeReference]</c> field: an ID into the object's reference registry.</summary>
public sealed record ScriptReference(string Type) : ScriptFieldType;

public sealed record ScriptField(string Name, ScriptFieldType Type);

/// <summary>A class or struct's serialized fields in order, base classes first.</summary>
public sealed record ScriptType(string Key, bool ValueType, IReadOnlyList<ScriptField> Fields);

/// <summary>
/// The serialized layouts of a player's script classes, from assembly metadata alone, by
/// Unity's rules. A field is serialized when it is an instance field that is not
/// <c>readonly</c>, <c>const</c> or <c>[NonSerialized]</c>, is public or marked
/// <c>[SerializeField]</c> or <c>[SerializeReference]</c>, and has a serializable type:
/// a primitive, <c>string</c>, an enum, a <c>UnityEngine.Object</c>, a Unity builtin value,
/// a non-abstract <c>[Serializable]</c> class or struct outside the .NET class library, or a
/// single array or <c>List&lt;T&gt;</c> of one of those. Byte alignment, builtin layouts and
/// Unity's nesting limit belong to the reader of the data.
/// </summary>
public sealed class ScriptLayouts : IDisposable
{
    private static readonly HashSet<string> Builtins = new(StringComparer.Ordinal)
    {
        "UnityEngine.Vector2", "UnityEngine.Vector3", "UnityEngine.Vector4", "UnityEngine.Vector2Int",
        "UnityEngine.Vector3Int", "UnityEngine.Quaternion", "UnityEngine.Matrix4x4", "UnityEngine.Color",
        "UnityEngine.Color32", "UnityEngine.Rect", "UnityEngine.RectInt", "UnityEngine.Bounds",
        "UnityEngine.BoundsInt", "UnityEngine.LayerMask", "UnityEngine.RenderingLayerMask", "UnityEngine.AnimationCurve", "UnityEngine.Gradient",
        "UnityEngine.RectOffset", "UnityEngine.GUIStyle", "UnityEngine.Hash128",
    };

    private readonly PlayerMetadata _metadata;
    private readonly Dictionary<string, ScriptType?> _types = new(StringComparer.Ordinal);
    // The declared types of [SerializeReference] fields, whose possible values need layouts.
    private readonly List<DefinedSig> _referenced = new();

    public ScriptLayouts(IEnumerable<(string Name, byte[] Image)> assemblies) => _metadata = new PlayerMetadata(assemblies);

    public void Dispose() => _metadata.Dispose();

    /// <summary>Every class and struct reached so far, by key.</summary>
    public IReadOnlyDictionary<string, ScriptType> Types =>
        _types.Where(entry => entry.Value is not null).ToDictionary(entry => entry.Key, entry => entry.Value!);

    /// <summary>The keys of an assembly's script classes: non-abstract, non-generic classes
    /// deriving from <c>UnityEngine.MonoBehaviour</c> or <c>UnityEngine.ScriptableObject</c>.
    /// Their layouts are added to <see cref="Types"/>.</summary>
    public IReadOnlyList<string> Scripts(string assembly)
    {
        var module = _metadata.Modules.FirstOrDefault(module => module.Name == assembly);
        if (module is null) return Array.Empty<string>();
        var keys = new List<string>();
        foreach (var handle in module.Reader.TypeDefinitions)
        {
            var type = new DefinedSig(new DefinedType(module, handle), ImmutableArray<SigType>.Empty);
            var definition = type.Type.Definition;
            if ((definition.Attributes & TypeAttributes.Abstract) != 0 || definition.GetGenericParameters().Count > 0) continue;
            if (Inherits(type, "UnityEngine.MonoBehaviour") || Inherits(type, "UnityEngine.ScriptableObject"))
                keys.Add(Composite(type));
        }
        return keys;
    }

    /// <summary>The key of a script class's layout, or null when the assembly does not define it.</summary>
    public string? Script(string assembly, string fullName) =>
        _metadata.Find(assembly, fullName) is { } type
            ? Composite(new DefinedSig(type, ImmutableArray<SigType>.Empty))
            : null;

    /// <summary>
    /// Adds the layouts of every class a <c>[SerializeReference]</c> field reached so far can
    /// hold: non-abstract, non-generic <c>[Serializable]</c> classes that derive from or
    /// implement the field's declared type. An object's reference registry names its values'
    /// classes, so their layouts must be known before the data is read.
    /// </summary>
    public void AddReferenceTargets()
    {
        for (var done = 0; done < _referenced.Count;)
        {
            var declared = _referenced.Skip(done).ToList();
            done = _referenced.Count;
            foreach (var module in _metadata.Modules)
                foreach (var handle in module.Reader.TypeDefinitions)
                {
                    var type = new DefinedSig(new DefinedType(module, handle), ImmutableArray<SigType>.Empty);
                    var definition = type.Type.Definition;
                    const TypeAttributes Excluded = TypeAttributes.Abstract | TypeAttributes.Interface;
                    if ((definition.Attributes & Excluded) != 0 || (definition.Attributes & Serializable) == 0) continue;
                    if (definition.GetGenericParameters().Count > 0 || IsValueType(type)) continue;
                    if (declared.Any(target => AssignableTo(type, target.Type))) Composite(type);
                }
        }
    }

    /// <summary>Whether a class derives from or implements <paramref name="target"/>.</summary>
    private bool AssignableTo(DefinedSig type, DefinedType target)
    {
        for (DefinedSig? level = type; level is not null; level = Base(level))
        {
            if (level.Type == target || Implements(level, target)) return true;
        }
        return false;
    }

    private bool Implements(DefinedSig type, DefinedType target)
    {
        var reader = type.Type.Reader;
        foreach (var handle in type.Type.Definition.GetInterfaceImplementations())
        {
            var implemented = _metadata.Resolve(type.Type.Module, reader.GetInterfaceImplementation(handle).Interface, type.Args);
            if (implemented is DefinedSig face && (face.Type == target || Implements(face, target))) return true;
        }
        return false;
    }

    /// <summary>Writes <c>{"scripts": [keys], "types": {key: {"value_type", "fields"}}}</c>,
    /// after adding the layouts <c>[SerializeReference]</c> values need.</summary>
    public void Write(Utf8JsonWriter writer, IEnumerable<string> scripts)
    {
        AddReferenceTargets();
        writer.WriteStartObject();
        writer.WriteStartArray("scripts");
        foreach (var key in scripts) writer.WriteStringValue(key);
        writer.WriteEndArray();
        writer.WriteStartObject("types");
        foreach (var (key, type) in Types.OrderBy(entry => entry.Key, StringComparer.Ordinal))
        {
            writer.WriteStartObject(key);
            writer.WriteBoolean("value_type", type.ValueType);
            writer.WriteStartArray("fields");
            foreach (var field in type.Fields)
            {
                writer.WriteStartObject();
                writer.WriteString("name", field.Name);
                writer.WritePropertyName("type");
                WriteType(writer, field.Type);
                writer.WriteEndObject();
            }
            writer.WriteEndArray();
            writer.WriteEndObject();
        }
        writer.WriteEndObject();
        writer.WriteEndObject();
    }

    private static void WriteType(Utf8JsonWriter writer, ScriptFieldType type)
    {
        writer.WriteStartObject();
        switch (type)
        {
            case ScriptPrimitive p: writer.WriteString("primitive", p.Name); break;
            case ScriptObject o: writer.WriteString("object", o.Type); break;
            case ScriptBuiltin b: writer.WriteString("builtin", b.Type); break;
            case ScriptComposite c: writer.WriteString("composite", c.Key); break;
            case ScriptReference r: writer.WriteString("reference", r.Type); break;
            case ScriptArray a:
                writer.WritePropertyName("array");
                WriteType(writer, a.Element);
                break;
        }
        writer.WriteEndObject();
    }

    /// <summary>Adds a class or struct's layout, if it is not known yet, and returns its key.</summary>
    private string Composite(DefinedSig type)
    {
        var key = Key(type);
        if (_types.ContainsKey(key)) return key;
        // A placeholder first, so a type that contains itself refers to its own key.
        _types[key] = null;
        var fields = new List<ScriptField>();
        foreach (var level in Chain(type).Reverse()) fields.AddRange(Fields(level));
        _types[key] = new ScriptType(key, IsValueType(type), fields);
        return key;
    }

    /// <summary>A type and its base classes whose fields a layout includes, most derived first.
    /// The chain stops at the .NET class library and at Unity's native object classes, whose
    /// fields (a component's game object, a script's name) the reader handles. Managed engine
    /// classes such as <c>UnityEventBase</c> contribute their fields.</summary>
    private IEnumerable<DefinedSig> Chain(DefinedSig type)
    {
        for (DefinedSig? level = type; level is not null; level = Base(level))
        {
            var assembly = level.Type.Assembly;
            if (IsClassLibrary(assembly) || NativeBases.Contains(level.Type.FullName))
                yield break;
            yield return level;
        }
    }

    /// <summary>Engine classes whose fields are native, wherever the assembly defining them comes from.</summary>
    private static readonly HashSet<string> NativeBases = new(StringComparer.Ordinal)
    {
        "UnityEngine.Object", "UnityEngine.Component", "UnityEngine.Behaviour",
        "UnityEngine.MonoBehaviour", "UnityEngine.ScriptableObject",
    };

    private IEnumerable<ScriptField> Fields(DefinedSig type)
    {
        var reader = type.Type.Reader;
        foreach (var handle in type.Type.Definition.GetFields())
        {
            var field = reader.GetFieldDefinition(handle);
            if (!Serialized(reader, field, out var reference)) continue;
            var declared = _metadata.FieldType(type.Type.Module, field, type.Args);
            if (Classify(declared, element: false, reference) is { } classified)
                yield return new ScriptField(reader.GetString(field.Name), classified);
        }
    }

    // [NonSerialized] and [Serializable] are metadata flags (ECMA-335 II.23.1.5, II.23.1.15);
    // .NET marks the enum members obsolete along with formatter serialization.
    private const FieldAttributes NotSerialized = (FieldAttributes)0x0080;
    private const TypeAttributes Serializable = (TypeAttributes)0x2000;

    private static bool Serialized(MetadataReader reader, FieldDefinition field, out bool reference)
    {
        reference = false;
        const FieldAttributes Excluded = FieldAttributes.Static | FieldAttributes.Literal | FieldAttributes.InitOnly | NotSerialized;
        if ((field.Attributes & Excluded) != 0) return false;
        var attributes = field.GetCustomAttributes().Select(handle => AttributeName(reader, reader.GetCustomAttribute(handle))).ToList();
        reference = attributes.Contains("UnityEngine.SerializeReference");
        return (field.Attributes & FieldAttributes.FieldAccessMask) == FieldAttributes.Public
            || reference
            || attributes.Contains("UnityEngine.SerializeField");
    }

    private ScriptFieldType? Classify(SigType type, bool element, bool reference) => type switch
    {
        PrimitiveSig primitive => PrimitiveName(primitive.Code) is { } name ? new ScriptPrimitive(name) : null,
        ArraySig array => element ? null : Elements(array.Element, reference),
        DefinedSig defined => ClassifyDefined(defined, element, reference),
        _ => null,
    };

    private ScriptFieldType? Elements(SigType element, bool reference) =>
        Classify(element, element: true, reference) is { } inner and not ScriptArray ? new ScriptArray(inner) : null;

    private ScriptFieldType? ClassifyDefined(DefinedSig type, bool element, bool reference)
    {
        var name = type.Type.FullName;
        if (IsClassLibrary(type.Type.Assembly))
            return name == "System.Collections.Generic.List`1" && type.Args.Length == 1 && !element
                ? Elements(type.Args[0], reference)
                : null;
        if (Inherits(type, "System.Enum")) return Classify(EnumUnderlying(type), element, reference: false);
        if (Inherits(type, "UnityEngine.Object")) return new ScriptObject(name);
        if (Builtins.Contains(name)) return new ScriptBuiltin(name);
        var definition = type.Type.Definition;
        if (reference)
        {
            if (IsValueType(type)) return null;
            if (!_referenced.Contains(type)) _referenced.Add(type);
            return new ScriptReference(name);
        }
        if ((definition.Attributes & (TypeAttributes.Interface | TypeAttributes.Abstract)) != 0) return null;
        if ((definition.Attributes & Serializable) == 0) return null;
        if (Inherits(type, "System.Delegate")) return null;
        if (type.Args.Length != definition.GetGenericParameters().Count || type.Args.Any(arg => arg is UnsupportedSig)) return null;
        return new ScriptComposite(Composite(type));
    }

    private SigType EnumUnderlying(DefinedSig type)
    {
        var reader = type.Type.Reader;
        foreach (var handle in type.Type.Definition.GetFields())
        {
            var field = reader.GetFieldDefinition(handle);
            if ((field.Attributes & FieldAttributes.Static) == 0)
                return _metadata.FieldType(type.Type.Module, field, type.Args);
        }
        return new UnsupportedSig("an enum without a value field");
    }

    private DefinedSig? Base(DefinedSig type)
    {
        var handle = type.Type.Definition.BaseType;
        if (handle.IsNil) return null;
        return _metadata.Resolve(type.Type.Module, handle, type.Args) as DefinedSig;
    }

    /// <summary>Whether the type is, or derives from, the type with this full name.</summary>
    private bool Inherits(DefinedSig type, string fullName)
    {
        for (DefinedSig? level = type; level is not null; level = Base(level))
            if (level.Type.FullName == fullName) return true;
        return false;
    }

    private bool IsValueType(DefinedSig type) =>
        Base(type) is { } parent && parent.Type.FullName is "System.ValueType" or "System.Enum";

    private static bool IsClassLibrary(string assembly) =>
        assembly is "mscorlib" or "netstandard" or "System" or "System.Runtime" or "System.Private.CoreLib"
        || assembly.StartsWith("System.", StringComparison.Ordinal)
        || assembly.StartsWith("Mono.", StringComparison.Ordinal);

    private static string? PrimitiveName(PrimitiveTypeCode code) => code switch
    {
        PrimitiveTypeCode.Boolean => "bool",
        PrimitiveTypeCode.SByte => "SInt8",
        PrimitiveTypeCode.Byte => "UInt8",
        PrimitiveTypeCode.Int16 => "SInt16",
        PrimitiveTypeCode.UInt16 => "UInt16",
        PrimitiveTypeCode.Char => "char",
        PrimitiveTypeCode.Int32 => "int",
        PrimitiveTypeCode.UInt32 => "unsigned int",
        PrimitiveTypeCode.Int64 => "SInt64",
        PrimitiveTypeCode.UInt64 => "UInt64",
        PrimitiveTypeCode.Single => "float",
        PrimitiveTypeCode.Double => "double",
        PrimitiveTypeCode.String => "string",
        _ => null,
    };

    /// <summary>A layout's key: the defining assembly and full name, with generic arguments.</summary>
    private static string Key(SigType type) => type switch
    {
        DefinedSig defined when defined.Args.IsEmpty => $"{defined.Type.Assembly}:{defined.Type.FullName}",
        DefinedSig defined => $"{defined.Type.Assembly}:{defined.Type.FullName}[{string.Join(",", defined.Args.Select(Key))}]",
        PrimitiveSig primitive => primitive.Code.ToString(),
        ArraySig array => Key(array.Element) + "[]",
        UnsupportedSig unsupported => unsupported.What,
        _ => "?",
    };

    private static string AttributeName(MetadataReader reader, CustomAttribute attribute)
    {
        var parent = attribute.Constructor.Kind switch
        {
            HandleKind.MemberReference => reader.GetMemberReference((MemberReferenceHandle)attribute.Constructor).Parent,
            HandleKind.MethodDefinition => reader.GetMethodDefinition((MethodDefinitionHandle)attribute.Constructor).GetDeclaringType(),
            _ => default(EntityHandle),
        };
        return parent.Kind switch
        {
            HandleKind.TypeReference => Name(reader, reader.GetTypeReference((TypeReferenceHandle)parent).Namespace, reader.GetTypeReference((TypeReferenceHandle)parent).Name),
            HandleKind.TypeDefinition => Name(reader, reader.GetTypeDefinition((TypeDefinitionHandle)parent).Namespace, reader.GetTypeDefinition((TypeDefinitionHandle)parent).Name),
            _ => "",
        };
    }

    private static string Name(MetadataReader reader, StringHandle space, StringHandle name)
    {
        var ns = reader.GetString(space);
        return ns.Length == 0 ? reader.GetString(name) : $"{ns}.{reader.GetString(name)}";
    }
}
