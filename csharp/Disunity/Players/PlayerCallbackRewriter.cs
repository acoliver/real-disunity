using Mono.Cecil;
using Mono.Cecil.Cil;

namespace Disunity.Hosting;

/// <summary>The outcome of <see cref="PlayerCallbackRewriter.Rewrite"/>.</summary>
/// <param name="Image">The rewritten assembly image, or null when the assembly is unchanged.</param>
/// <param name="Sites">How many delegate constructions were replaced.</param>
/// <param name="Failure">
/// Why an assembly with sites could not be rewritten (Cecil needs a type that no assembly in the search directory
/// defines), or null.
/// </param>
public sealed record CallbackRewrite(byte[]? Image, int Sites, string? Failure = null);

/// <summary>
/// Rewrites a game assembly so the delegates it makes for static methods marked <c>MonoPInvokeCallback</c> are
/// created once and kept (<see cref="NativeCallbacks"/>). Unity games mark every method they hand to native code
/// that way, and each C# binding defines its own attribute class, so the marker is the attribute's type name in any
/// namespace. A site is <c>ldnull; ldftn M; newobj D::.ctor(object, native int)</c> with a static, marked <c>M</c>
/// defined in the same assembly; the <c>newobj</c> becomes <c>call NativeCallbacks.Keep&lt;D&gt;(object, native int)</c>,
/// which takes the same operands and leaves the same delegate on the stack.
/// </summary>
public static class PlayerCallbackRewriter
{
    private const string MarkerName = "MonoPInvokeCallbackAttribute";

    private static readonly CallbackRewrite Unchanged = new(null, 0);

    /// <summary>
    /// Rewrites the sites in <paramref name="image"/>. The result has no image when the assembly has no site, is not
    /// a managed image that can be rewritten (mixed-mode assemblies are left alone), or cannot be read. Writing an
    /// assembly can need the definition of a type it references, such as the enum type of a parameter default, so
    /// <paramref name="searchDirectory"/> (the directory the assembly came from) is searched for the assemblies that
    /// define them. An assembly whose types cannot be found is left unchanged, with the reason in
    /// <see cref="CallbackRewrite.Failure"/>.
    /// </summary>
    public static CallbackRewrite Rewrite(byte[] image, string? searchDirectory = null)
    {
        using var resolver = new DefaultAssemblyResolver();
        if (searchDirectory is not null) resolver.AddSearchDirectory(searchDirectory);
        ModuleDefinition module;
        try
        {
            module = ModuleDefinition.ReadModule(new MemoryStream(image), new ReaderParameters { AssemblyResolver = resolver });
        }
        catch (Exception error) when (error is BadImageFormatException or ArgumentException or IOException or InvalidOperationException)
        {
            return Unchanged;
        }
        using (module)
        {
            if (!module.Attributes.HasFlag(ModuleAttributes.ILOnly)) return Unchanged;
            var sites = Sites(module).ToList();
            if (sites.Count == 0) return Unchanged;
            var keep = KeepMethod(module);
            foreach (var (instruction, delegateType) in sites)
            {
                var instance = new GenericInstanceMethod(keep);
                instance.GenericArguments.Add(delegateType);
                instruction.OpCode = OpCodes.Call;
                instruction.Operand = instance;
            }
            try
            {
                using var output = new MemoryStream();
                module.Write(output);
                return new CallbackRewrite(output.ToArray(), sites.Count);
            }
            catch (Exception error) when (error is ResolutionException or AssemblyResolutionException)
            {
                return new CallbackRewrite(null, sites.Count, error.Message);
            }
        }
    }

    private static IEnumerable<(Instruction Create, TypeReference Delegate)> Sites(ModuleDefinition module)
    {
        foreach (var type in module.GetTypes())
        {
            foreach (var method in type.Methods.Where(m => m.HasBody))
            {
                var code = method.Body.Instructions;
                for (var i = 1; i + 1 < code.Count; i++)
                {
                    if (IsSite(code[i - 1], code[i], code[i + 1], out var delegateType)) yield return (code[i + 1], delegateType);
                }
            }
        }
    }

    private static bool IsSite(Instruction target, Instruction pointer, Instruction create, out TypeReference delegateType)
    {
        delegateType = null!;
        if (target.OpCode != OpCodes.Ldnull || pointer.OpCode != OpCodes.Ldftn || create.OpCode != OpCodes.Newobj) return false;
        if (pointer.Operand is not MethodDefinition { IsStatic: true } callback || !IsMarked(callback)) return false;
        if (create.Operand is not MethodReference { Name: ".ctor", Parameters.Count: 2 } constructor) return false;
        if (constructor.Parameters[0].ParameterType.FullName != "System.Object" || constructor.Parameters[1].ParameterType.FullName != "System.IntPtr")
            return false;
        // A delegate type built from the enclosing method's own generic parameters has no single instance to keep.
        if (constructor.DeclaringType.ContainsGenericParameter) return false;
        delegateType = constructor.DeclaringType;
        return true;
    }

    private static bool IsMarked(MethodDefinition method) =>
        method.HasCustomAttributes && method.CustomAttributes.Any(a => a.AttributeType.Name == MarkerName);

    /// <summary>A reference to <c>NativeCallbacks.Keep&lt;T&gt;(object, native int)</c> returning <c>T</c>.</summary>
    private static MethodReference KeepMethod(ModuleDefinition module)
    {
        var engine = typeof(NativeCallbacks).Assembly.GetName();
        var scope = module.AssemblyReferences.FirstOrDefault(r => r.Name == engine.Name);
        if (scope is null)
        {
            scope = new AssemblyNameReference(engine.Name, engine.Version);
            var token = engine.GetPublicKeyToken();
            if (token is { Length: > 0 }) scope.PublicKeyToken = token;
            module.AssemblyReferences.Add(scope);
        }
        var owner = new TypeReference(typeof(NativeCallbacks).Namespace, nameof(NativeCallbacks), module, scope);
        var keep = new MethodReference(nameof(NativeCallbacks.Keep), module.TypeSystem.Void, owner);
        keep.GenericParameters.Add(new GenericParameter(keep));
        keep.ReturnType = keep.GenericParameters[0];
        keep.Parameters.Add(new ParameterDefinition(module.TypeSystem.Object));
        keep.Parameters.Add(new ParameterDefinition(module.TypeSystem.IntPtr));
        return keep;
    }
}
