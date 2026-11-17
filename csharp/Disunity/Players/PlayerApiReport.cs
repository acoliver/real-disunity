using System.Collections.Immutable;
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
/// their metadata alone. A type counts as present when a facade forwards it
/// (<see cref="ModuleFacades.Forwards"/>). Members match their declared field or method
/// signatures, including generic parameters and constructed declaring types.
/// This is a reference report, not a check of IL access rules or generic constraints.
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
        var signatures = new SignatureProvider(target);
        foreach (var handle in reader.TypeReferences)
        {
            var reference = signatures.GetTypeFromReference(reader, handle, 0);
            if (reference.Module is { } module && ModuleFacades.IsEngineModule(module)
                && !ModuleFacades.Forwards(target, reference.Definition!) && OuterIsForwarded(target, reference.Definition!))
                yield return (module, reference.Definition!, "");
        }
        foreach (var handle in reader.MemberReferences)
        {
            var member = reader.GetMemberReference(handle);
            if (signatures.Resolve(reader, member.Parent) is not { Module: { } module, Definition: { } type } parent
                || !ModuleFacades.IsEngineModule(module) || !ModuleFacades.Forwards(target, type)) continue;
            var found = target.GetType(type)!;
            var name = reader.GetString(member.Name);
            if (!HasMember(found, name, member, reader, signatures))
                yield return (module, parent.Display!, Describe(name, member, reader, signatures));
        }
    }

    private static bool OuterIsForwarded(Assembly target, string type)
    {
        var split = type.LastIndexOf('+');
        return split < 0 || ModuleFacades.Forwards(target, type[..split]);
    }

    private static readonly Comparer<(string Module, string Type, string Member)> Ordinal =
        Comparer<(string Module, string Type, string Member)>.Create((a, b) =>
        {
            var order = string.CompareOrdinal(a.Module, b.Module);
            if (order == 0) order = string.CompareOrdinal(a.Type, b.Type);
            return order != 0 ? order : string.CompareOrdinal(a.Member, b.Member);
        });

    private const BindingFlags Everything =
        BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static;

    private static bool HasMember(Type type, string name, MemberReference member, MetadataReader reader, SignatureProvider provider)
    {
        if (member.GetKind() == MemberReferenceKind.Field)
        {
            var signature = member.DecodeFieldSignature(provider, null).Key;
            return type.GetFields(Everything).Any(f => f.Name == name
                && provider.Modified(f.FieldType, f.GetRequiredCustomModifiers()) == signature);
        }
        var expected = member.DecodeMethodSignature(provider, null);
        IEnumerable<MethodBase> candidates = name switch
        {
            ".ctor" => type.GetConstructors(Everything).Where(c => !c.IsStatic),
            ".cctor" => type.TypeInitializer is { } initializer ? [initializer] : [],
            _ => type.GetMethods(Everything).Where(m => m.Name == name),
        };
        return candidates.Any(candidate => Matches(candidate, expected, provider));
    }

    private static bool Matches(MethodBase candidate, MethodSignature<ReportType> expected, SignatureProvider provider)
    {
        if (candidate.IsStatic == expected.Header.IsInstance
            || candidate.CallingConvention.HasFlag(CallingConventions.ExplicitThis) != expected.Header.HasExplicitThis
            || candidate.CallingConvention.HasFlag(CallingConventions.VarArgs)
                != (expected.Header.CallingConvention == SignatureCallingConvention.VarArgs)) return false;
        var method = candidate as MethodInfo;
        if ((method?.GetGenericArguments().Length ?? 0) != expected.GenericParameterCount) return false;
        var returned = method is null ? provider.ReflectionType(typeof(void)) : provider.Parameter(method.ReturnParameter);
        return returned == expected.ReturnType.Key
            && candidate.GetParameters().Select(provider.Parameter)
                .SequenceEqual(expected.ParameterTypes.Take(expected.RequiredParameterCount).Select(p => p.Key));
    }

    private static string Describe(string name, MemberReference member, MetadataReader reader, SignatureProvider provider)
    {
        if (member.GetKind() == MemberReferenceKind.Field)
            return $"{name}: {member.DecodeFieldSignature(provider, null).Key}";
        var signature = member.DecodeMethodSignature(provider, null);
        var generic = signature.GenericParameterCount == 0 ? "" : $"``{signature.GenericParameterCount}";
        var kind = signature.Header.IsInstance ? "instance" : "static";
        return $"{kind} {name}{generic}({string.Join(", ", signature.ParameterTypes.Select(p => p.Key))}): {signature.ReturnType.Key}";
    }

    private readonly record struct ReportType(string Key, string? Module = null, string? Definition = null, string? Display = null);

    private sealed class SignatureProvider(Assembly target) : ISignatureTypeProvider<ReportType, object?>
    {
        public ReportType? Resolve(MetadataReader reader, EntityHandle handle) => handle.Kind switch
        {
            HandleKind.TypeReference => GetTypeFromReference(reader, (TypeReferenceHandle)handle, 0),
            HandleKind.TypeSpecification => GetTypeFromSpecification(reader, null, (TypeSpecificationHandle)handle, 0),
            _ => null,
        };

        public string Parameter(ParameterInfo parameter) => Modified(parameter.ParameterType, parameter.GetRequiredCustomModifiers());

        public string Modified(Type type, Type[] modifiers) =>
            string.Concat(modifiers.Select(m => $"modreq({ReflectionType(m)}) ")) + ReflectionType(type);

        public string ReflectionType(Type type)
        {
            if (type.IsGenericParameter) return (type.DeclaringMethod is null ? "!" : "!!") + type.GenericParameterPosition;
            if (type.IsByRef) return ReflectionType(type.GetElementType()!) + "&";
            if (type.IsPointer) return ReflectionType(type.GetElementType()!) + "*";
            if (type.IsArray) return ReflectionType(type.GetElementType()!) + ArraySuffix(type.GetArrayRank(), type.IsSZArray);
            if (type.IsFunctionPointer) return FunctionPointer(type);
            if (type.IsConstructedGenericType)
                return ReflectionType(type.GetGenericTypeDefinition()) + "<" + string.Join(", ", type.GetGenericArguments().Select(ReflectionType)) + ">";
            return Identity(type.Assembly.GetName().Name!, type.FullName!);
        }

        private static string ArraySuffix(int rank, bool vector) => vector ? "[]" : rank == 1 ? "[*]" : "[" + new string(',', rank - 1) + "]";

        private string Identity(string module, string name)
        {
            if (ModuleFacades.IsEngineModule(module)) module = target.GetName().Name!;
            if (module == typeof(object).Assembly.GetName().Name) return name;
            if (module is "mscorlib" or "System.Runtime" or "netstandard" or "System.Collections"
                && typeof(object).Assembly.GetType(name) is not null) return name;
            return $"[{module}]{name}";
        }

        public ReportType GetTypeFromReference(MetadataReader reader, TypeReferenceHandle handle, byte rawTypeKind)
        {
            var reference = reader.GetTypeReference(handle);
            var name = reader.GetString(reference.Name);
            if (reference.ResolutionScope.Kind == HandleKind.TypeReference)
            {
                var outer = GetTypeFromReference(reader, (TypeReferenceHandle)reference.ResolutionScope, rawTypeKind);
                return Named(outer.Module!, outer.Definition + "+" + name);
            }
            var module = reference.ResolutionScope.Kind switch
            {
                HandleKind.AssemblyReference => reader.GetString(reader.GetAssemblyReference((AssemblyReferenceHandle)reference.ResolutionScope).Name),
                HandleKind.ModuleDefinition => reader.GetString(reader.GetAssemblyDefinition().Name),
                _ => throw new BadImageFormatException("API signature type reference has no assembly scope"),
            };
            var ns = reader.GetString(reference.Namespace);
            return Named(module, ns.Length == 0 ? name : ns + "." + name);
        }

        public ReportType GetTypeFromDefinition(MetadataReader reader, TypeDefinitionHandle handle, byte rawTypeKind)
        {
            var definition = reader.GetTypeDefinition(handle);
            var name = reader.GetString(definition.Name);
            if (definition.GetDeclaringType() is { IsNil: false } outer)
            {
                var enclosing = GetTypeFromDefinition(reader, outer, rawTypeKind);
                return Named(enclosing.Module!, enclosing.Definition + "+" + name);
            }
            var ns = reader.GetString(definition.Namespace);
            return Named(reader.GetString(reader.GetAssemblyDefinition().Name), ns.Length == 0 ? name : ns + "." + name);
        }

        private ReportType Named(string module, string name) => new(Identity(module, name), module, name, name);

        public ReportType GetTypeFromSpecification(MetadataReader reader, object? context, TypeSpecificationHandle handle, byte rawTypeKind) =>
            reader.GetTypeSpecification(handle).DecodeSignature(this, context);

        public ReportType GetGenericInstantiation(ReportType genericType, ImmutableArray<ReportType> typeArguments)
        {
            var arguments = "<" + string.Join(", ", typeArguments.Select(a => a.Key)) + ">";
            return genericType with { Key = genericType.Key + arguments, Display = genericType.Display + arguments };
        }

        public ReportType GetGenericTypeParameter(object? genericContext, int index) => new("!" + index);
        public ReportType GetGenericMethodParameter(object? genericContext, int index) => new("!!" + index);
        public ReportType GetSZArrayType(ReportType elementType) => new(elementType.Key + "[]");
        public ReportType GetArrayType(ReportType elementType, ArrayShape shape) => new(elementType.Key + ArraySuffix(shape.Rank, false));
        public ReportType GetByReferenceType(ReportType elementType) => new(elementType.Key + "&");
        public ReportType GetPointerType(ReportType elementType) => new(elementType.Key + "*");
        public ReportType GetModifiedType(ReportType modifier, ReportType unmodifiedType, bool isRequired) =>
            isRequired ? new($"modreq({modifier.Key}) {unmodifiedType.Key}") : unmodifiedType;
        public ReportType GetPinnedType(ReportType elementType) => elementType;
        public ReportType GetFunctionPointerType(MethodSignature<ReportType> signature) =>
            new($"fnptr[{signature.Header.CallingConvention}]({string.Join(", ", signature.ParameterTypes.Select(p => p.Key))}): {signature.ReturnType.Key}");

        private string FunctionPointer(Type type)
        {
            var convention = type.IsUnmanagedFunctionPointer ? SignatureCallingConvention.Unmanaged : SignatureCallingConvention.Default;
            var conventions = type.GetFunctionPointerCallingConventions();
            if (conventions.Length != 0)
                convention = conventions[0].Name switch
                {
                    "CallConvCdecl" => SignatureCallingConvention.CDecl,
                    "CallConvStdcall" => SignatureCallingConvention.StdCall,
                    "CallConvThiscall" => SignatureCallingConvention.ThisCall,
                    "CallConvFastcall" => SignatureCallingConvention.FastCall,
                    _ => convention,
                };
            return $"fnptr[{convention}]({string.Join(", ", type.GetFunctionPointerParameterTypes().Select(ReflectionType))}): {ReflectionType(type.GetFunctionPointerReturnType())}";
        }

        public ReportType GetPrimitiveType(PrimitiveTypeCode typeCode) => new("System." + typeCode);
    }
}
