using Mono.Cecil;
using Mono.Cecil.Cil;
using TypeAttributes = Mono.Cecil.TypeAttributes;
using FieldAttributes = Mono.Cecil.FieldAttributes;
using ParameterAttributes = Mono.Cecil.ParameterAttributes;
using MethodAttributes = Mono.Cecil.MethodAttributes;
using MethodImplAttributes = Mono.Cecil.MethodImplAttributes;

/// <summary>
/// Builds a small game assembly with Cecil for the native callback tests: a delegate type
/// <c>Game.Callback(int) -> int</c>, a locally defined <c>MonoPInvokeCallbackAttribute</c> (the games each define
/// their own), and <c>Game.Holder</c> with these members:
/// <c>Marked</c> (static, marked, adds 1), <c>Plain</c> (static, unmarked, adds 2),
/// <c>MarkedInstance</c> (instance, marked), and the factories <c>Make</c> (<c>ldnull; ldftn Marked</c>),
/// <c>MakePlain</c>, <c>MakeInstance</c> (<c>ldarg.0; ldftn MarkedInstance</c>, an instance target) and
/// <c>MakeNullInstance</c> (<c>ldnull; ldftn MarkedInstance</c>) and <c>MakeClosed</c> (<c>ldarg.0; ldftn Marked</c>, a
/// static method with a target), each ending in <c>newobj Callback::.ctor</c>. The last three are only inspected.
/// </summary>
internal static class CallbackAssembly
{
    public static byte[] Build(bool marked = true, bool enumDefault = false)
    {
        // Writing the enum default needs Dep, which only exists in memory here.
        var resolver = new DependencyResolver(DefineDependency());
        var parameters = new ModuleParameters { Kind = ModuleKind.Dll, AssemblyResolver = resolver };
        var assembly = AssemblyDefinition.CreateAssembly(new AssemblyNameDefinition("Game", new Version(1, 0, 0, 0)), "Game", parameters);
        var module = assembly.MainModule;
        var attribute = DefineAttribute(module);
        var callback = DefineDelegate(module);
        var holder = new TypeDefinition("Game", "Holder", TypeAttributes.Public | TypeAttributes.Class, module.ImportReference(typeof(object)));
        module.Types.Add(holder);
        var marker = marked ? attribute.Methods.Single(m => m.IsConstructor) : null;
        var markedMethod = AddMethod(holder, "Marked", isStatic: true, marker, 1);
        var plain = AddMethod(holder, "Plain", isStatic: true, null, 2);
        var markedInstance = AddMethod(holder, "MarkedInstance", isStatic: false, marker, 3);
        var constructor = callback.Methods.Single(m => m.IsConstructor);
        AddFactory(holder, "Make", callback, constructor, OpCodes.Ldnull, markedMethod);
        AddFactory(holder, "MakePlain", callback, constructor, OpCodes.Ldnull, plain);
        AddFactory(holder, "MakeNullInstance", callback, constructor, OpCodes.Ldnull, markedInstance);
        AddFactory(holder, "MakeInstance", callback, constructor, OpCodes.Ldarg_0, markedInstance, takesHolder: true);
        AddFactory(holder, "MakeClosed", callback, constructor, OpCodes.Ldarg_0, markedMethod, takesHolder: true);
        if (enumDefault) AddEnumDefault(holder);
        return Write(assembly);
    }

    /// <summary>
    /// The assembly <c>Dep</c>, which defines the enum <c>Dep.Mode</c> that <c>Build(enumDefault: true)</c> uses as a
    /// parameter default. Writing the game needs the enum's underlying type, so it needs <c>Dep.dll</c> nearby.
    /// </summary>
    public static byte[] BuildDependency() => Write(DefineDependency());

    private static AssemblyDefinition DefineDependency()
    {
        var assembly = AssemblyDefinition.CreateAssembly(new AssemblyNameDefinition("Dep", new Version(1, 0, 0, 0)), "Dep", ModuleKind.Dll);
        var module = assembly.MainModule;
        var mode = new TypeDefinition("Dep", "Mode",
            TypeAttributes.Public | TypeAttributes.Sealed | TypeAttributes.Class, module.ImportReference(typeof(Enum)));
        mode.Fields.Add(new FieldDefinition("value__", FieldAttributes.Public | FieldAttributes.SpecialName | FieldAttributes.RTSpecialName,
            module.ImportReference(typeof(int))));
        mode.Fields.Add(new FieldDefinition("A", FieldAttributes.Public | FieldAttributes.Static | FieldAttributes.Literal | FieldAttributes.HasDefault,
            mode) { Constant = 1 });
        module.Types.Add(mode);
        return assembly;
    }

    private static void AddEnumDefault(TypeDefinition holder)
    {
        var module = holder.Module;
        var scope = new AssemblyNameReference("Dep", new Version(1, 0, 0, 0));
        module.AssemblyReferences.Add(scope);
        var mode = new TypeReference("Dep", "Mode", module, scope, valueType: true);
        var method = new MethodDefinition("WithDefault", MethodAttributes.Public | MethodAttributes.HideBySig | MethodAttributes.Static,
            module.ImportReference(typeof(void)));
        method.Parameters.Add(new ParameterDefinition("mode", ParameterAttributes.Optional | ParameterAttributes.HasDefault, mode) { Constant = 1 });
        method.Body.GetILProcessor().Emit(OpCodes.Ret);
        holder.Methods.Add(method);
    }

    private static byte[] Write(AssemblyDefinition assembly)
    {
        using var stream = new MemoryStream();
        assembly.Write(stream);
        return stream.ToArray();
    }

    private static TypeDefinition DefineAttribute(ModuleDefinition module)
    {
        var baseType = module.ImportReference(typeof(Attribute));
        var type = new TypeDefinition("Game", "MonoPInvokeCallbackAttribute", TypeAttributes.Public | TypeAttributes.Class, baseType);
        var constructor = new MethodDefinition(".ctor",
            MethodAttributes.Public | MethodAttributes.HideBySig | MethodAttributes.SpecialName | MethodAttributes.RTSpecialName,
            module.ImportReference(typeof(void)));
        var il = constructor.Body.GetILProcessor();
        il.Emit(OpCodes.Ldarg_0);
        il.Emit(OpCodes.Call, module.ImportReference(typeof(Attribute).GetConstructor(
            System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Public, Type.EmptyTypes)));
        il.Emit(OpCodes.Ret);
        type.Methods.Add(constructor);
        module.Types.Add(type);
        return type;
    }

    private static TypeDefinition DefineDelegate(ModuleDefinition module)
    {
        var type = new TypeDefinition("Game", "Callback",
            TypeAttributes.Public | TypeAttributes.Sealed | TypeAttributes.Class, module.ImportReference(typeof(MulticastDelegate)));
        var runtime = MethodImplAttributes.Runtime | MethodImplAttributes.Managed;
        var constructor = new MethodDefinition(".ctor",
            MethodAttributes.Public | MethodAttributes.HideBySig | MethodAttributes.SpecialName | MethodAttributes.RTSpecialName,
            module.ImportReference(typeof(void))) { ImplAttributes = runtime };
        constructor.Parameters.Add(new ParameterDefinition(module.ImportReference(typeof(object))));
        constructor.Parameters.Add(new ParameterDefinition(module.ImportReference(typeof(IntPtr))));
        var invoke = new MethodDefinition("Invoke",
            MethodAttributes.Public | MethodAttributes.HideBySig | MethodAttributes.NewSlot | MethodAttributes.Virtual,
            module.ImportReference(typeof(int))) { ImplAttributes = runtime };
        invoke.Parameters.Add(new ParameterDefinition(module.ImportReference(typeof(int))));
        type.Methods.Add(constructor);
        type.Methods.Add(invoke);
        module.Types.Add(type);
        return type;
    }

    /// <summary>A method <c>int name(int x)</c> returning x + amount, with the marker attribute when one is given.</summary>
    private static MethodDefinition AddMethod(TypeDefinition type, string name, bool isStatic, MethodDefinition? marker, int amount)
    {
        var module = type.Module;
        var attributes = MethodAttributes.Public | MethodAttributes.HideBySig | (isStatic ? MethodAttributes.Static : 0);
        var method = new MethodDefinition(name, attributes, module.ImportReference(typeof(int)));
        var argument = new ParameterDefinition(module.ImportReference(typeof(int)));
        method.Parameters.Add(argument);
        var il = method.Body.GetILProcessor();
        il.Emit(OpCodes.Ldarg, argument);
        il.Emit(OpCodes.Ldc_I4, amount);
        il.Emit(OpCodes.Add);
        il.Emit(OpCodes.Ret);
        if (marker is not null) method.CustomAttributes.Add(new CustomAttribute(marker));
        type.Methods.Add(method);
        return method;
    }

    private static void AddFactory(TypeDefinition type, string name, TypeDefinition callback, MethodDefinition constructor,
        OpCode target, MethodDefinition method, bool takesHolder = false)
    {
        var factory = new MethodDefinition(name, MethodAttributes.Public | MethodAttributes.HideBySig | MethodAttributes.Static, callback);
        if (takesHolder) factory.Parameters.Add(new ParameterDefinition(type));
        var il = factory.Body.GetILProcessor();
        il.Emit(target);
        il.Emit(OpCodes.Ldftn, method);
        il.Emit(OpCodes.Newobj, constructor);
        il.Emit(OpCodes.Ret);
        type.Methods.Add(factory);
    }
}

/// <summary>Resolves the one assembly the builder defines in memory.</summary>
internal sealed class DependencyResolver(AssemblyDefinition dependency) : IAssemblyResolver
{
    public AssemblyDefinition Resolve(AssemblyNameReference name) => Resolve(name, new ReaderParameters());

    public AssemblyDefinition Resolve(AssemblyNameReference name, ReaderParameters parameters) =>
        name.Name == dependency.Name.Name ? dependency : throw new AssemblyResolutionException(name);

    public void Dispose() { }
}
