using Mono.Cecil;
using Mono.Cecil.Cil;
using TypeAttributes = Mono.Cecil.TypeAttributes;
using MethodAttributes = Mono.Cecil.MethodAttributes;

/// <summary>
/// Builds, with Cecil, the shape of a player assembly whose generated job registration code instantiates a public
/// generic method with a type its own assembly keeps private: <c>Jobs.Extensions.Init&lt;T&gt;() where T : struct</c>
/// (public, in a public class), <c>Jobs.Outer</c> with a private nested struct <c>Hidden</c>, and the top-level class
/// <c>__JobReflectionRegistrationOutput__1</c> whose static <c>Register</c> calls <c>Init&lt;Outer.Hidden&gt;()</c>.
/// </summary>
internal static class JobRegistrationAssembly
{
    public const string AssemblyName = "JobGame";

    public static byte[] Build(bool withRegistration = true)
    {
        var assembly = AssemblyDefinition.CreateAssembly(new AssemblyNameDefinition(AssemblyName, new Version(1, 0, 0, 0)), AssemblyName, ModuleKind.Dll);
        Define(assembly.MainModule, withRegistration);
        using var stream = new MemoryStream();
        assembly.Write(stream);
        return stream.ToArray();
    }

    /// <summary>Adds the types to <paramref name="module"/>.</summary>
    public static void Define(ModuleDefinition module) => Define(module, withRegistration: true);

    private static void Define(ModuleDefinition module, bool withRegistration)
    {
        var init = DefineInit(module);
        var hidden = DefineOuter(module);
        if (withRegistration) DefineRegistration(module, init, hidden);
    }

    private static MethodDefinition DefineInit(ModuleDefinition module)
    {
        var type = new TypeDefinition("Jobs", "Extensions",
            TypeAttributes.Public | TypeAttributes.Abstract | TypeAttributes.Sealed | TypeAttributes.Class, module.ImportReference(typeof(object)));
        var init = new MethodDefinition("Init", MethodAttributes.Public | MethodAttributes.HideBySig | MethodAttributes.Static,
            module.TypeSystem.Void);
        init.GenericParameters.Add(new GenericParameter("T", init) { Attributes = GenericParameterAttributes.NotNullableValueTypeConstraint });
        init.Body.GetILProcessor().Emit(OpCodes.Ret);
        type.Methods.Add(init);
        module.Types.Add(type);
        return init;
    }

    private static TypeDefinition DefineOuter(ModuleDefinition module)
    {
        var outer = new TypeDefinition("Jobs", "Outer", TypeAttributes.Public | TypeAttributes.Class, module.ImportReference(typeof(object)));
        var hidden = new TypeDefinition(null, "Hidden",
            TypeAttributes.NestedPrivate | TypeAttributes.Sealed | TypeAttributes.SequentialLayout, module.ImportReference(typeof(ValueType)));
        outer.NestedTypes.Add(hidden);
        module.Types.Add(outer);
        return hidden;
    }

    private static void DefineRegistration(ModuleDefinition module, MethodDefinition init, TypeDefinition hidden)
    {
        var type = new TypeDefinition("", "__JobReflectionRegistrationOutput__1",
            TypeAttributes.NotPublic | TypeAttributes.Abstract | TypeAttributes.Sealed | TypeAttributes.Class, module.ImportReference(typeof(object)));
        var register = new MethodDefinition("Register", MethodAttributes.Public | MethodAttributes.HideBySig | MethodAttributes.Static,
            module.TypeSystem.Void);
        var call = new GenericInstanceMethod(init);
        call.GenericArguments.Add(hidden);
        var il = register.Body.GetILProcessor();
        il.Emit(OpCodes.Call, call);
        il.Emit(OpCodes.Ret);
        type.Methods.Add(register);
        module.Types.Add(type);
    }
}
