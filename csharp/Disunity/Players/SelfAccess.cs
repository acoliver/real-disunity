using Mono.Cecil;
using Mono.Cecil.Cil;

namespace Disunity.Hosting;

/// <summary>
/// Lets a player's job registration code use the assembly's own private types as type arguments. The assembly
/// generates a top-level class named <c>__JobReflectionRegistrationOutput__...</c> that instantiates the job package's
/// public generic methods with job types that the assembly declares non-public. A player's own runtime allows that;
/// this host's runtime checks that the calling type can see each type argument and throws
/// <see cref="MethodAccessException"/>. The runtime skips access checks into any assembly named by an assembly-level
/// <c>System.Runtime.CompilerServices.IgnoresAccessChecksToAttribute</c>, matched by the attribute's full name, so an
/// assembly that names itself no longer fails them. Each assembly carries its own definition of the attribute.
/// </summary>
internal static class SelfAccess
{
    private const string Namespace = "System.Runtime.CompilerServices";
    private const string AttributeName = "IgnoresAccessChecksToAttribute";
    private const string RegistrationPrefix = "__JobReflectionRegistrationOutput__";

    /// <summary>True when the module holds job registration code and does not name itself yet.</summary>
    public static bool IsNeeded(ModuleDefinition module) =>
        module.GetTypes().Any(type => type.Name.StartsWith(RegistrationPrefix, StringComparison.Ordinal)) && !NamesItself(module.Assembly);

    private static bool NamesItself(AssemblyDefinition assembly) => assembly.CustomAttributes.Any(attribute =>
        attribute.AttributeType is { Name: AttributeName, Namespace: Namespace }
        && attribute.ConstructorArguments is [{ Value: string name }]
        && name == assembly.Name.Name);

    /// <summary>Adds the attribute type to the module when it has none, and an assembly attribute naming the module's assembly.</summary>
    public static void Grant(ModuleDefinition module)
    {
        var constructor = module.Types.FirstOrDefault(type => type.Namespace == Namespace && type.Name == AttributeName) is { } existing
            ? existing.Methods.FirstOrDefault(method => method.IsConstructor && !method.IsStatic && method.Parameters is [{ ParameterType.FullName: "System.String" }])
            : DefineAttribute(module);
        if (constructor is null) return;
        var attribute = new CustomAttribute(constructor);
        attribute.ConstructorArguments.Add(new CustomAttributeArgument(module.TypeSystem.String, module.Assembly.Name.Name));
        module.Assembly.CustomAttributes.Add(attribute);
    }

    /// <summary>
    /// <c>public sealed class IgnoresAccessChecksToAttribute : Attribute</c> with a constructor taking the assembly's
    /// name. Its base type comes from the module's own core library, so an assembly built for another framework still
    /// refers to its own.
    /// </summary>
    private static MethodDefinition DefineAttribute(ModuleDefinition module)
    {
        var baseType = new TypeReference("System", "Attribute", module, module.TypeSystem.CoreLibrary);
        var type = new TypeDefinition(Namespace, AttributeName,
            TypeAttributes.Public | TypeAttributes.Sealed | TypeAttributes.BeforeFieldInit | TypeAttributes.Class, baseType);
        var baseConstructor = new MethodReference(".ctor", module.TypeSystem.Void, baseType) { HasThis = true };
        var constructor = new MethodDefinition(".ctor",
            MethodAttributes.Public | MethodAttributes.HideBySig | MethodAttributes.SpecialName | MethodAttributes.RTSpecialName,
            module.TypeSystem.Void);
        constructor.Parameters.Add(new ParameterDefinition("assemblyName", ParameterAttributes.None, module.TypeSystem.String));
        var il = constructor.Body.GetILProcessor();
        il.Emit(OpCodes.Ldarg_0);
        il.Emit(OpCodes.Call, baseConstructor);
        il.Emit(OpCodes.Ret);
        type.Methods.Add(constructor);
        module.Types.Add(type);
        return constructor;
    }
}
