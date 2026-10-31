using System.Reflection;
using System.Reflection.Emit;
using System.Text;
using UnityEngine;

/// <summary>
/// Scripts whose RequireComponent attribute names a type the engine lacks, in an assembly that is not deployed or in the engine itself. The attribute blob is written by hand
/// because the compiler cannot emit a reference to a type that does not exist; reading the attribute throws.
/// </summary>
internal static class UnresolvableRequirementTypes
{
    public const string MissingAssembly = "Missing.Requirement, Missing.Assembly.NotDeployed";

    /// <summary>A type that a loaded assembly lacks, the failure a script gets when the engine does not have a type it names.</summary>
    public static readonly string MissingType = $"UnityEngine.Missing.Requirement, {typeof(Camera).Assembly.FullName}";

    /// <summary>A script whose own attribute is unresolvable and whose base script requires a camera.</summary>
    public static Type BrokenOverGood { get; private set; } = null!;

    /// <summary>A script whose base script's attribute is unresolvable and whose own attribute requires a camera.</summary>
    public static Type GoodOverBroken { get; private set; } = null!;

    public static void Build()
    {
        var module = AssemblyBuilder.DefineDynamicAssembly(new AssemblyName("RequireComponentFixtures"), AssemblyBuilderAccess.Run)
            .DefineDynamicModule("RequireComponentFixtures");
        var good = Script(module, "Good", typeof(MonoBehaviour), typeof(Camera).AssemblyQualifiedName!);
        var broken = Script(module, "Broken", typeof(MonoBehaviour), MissingType);
        BrokenOverGood = Script(module, "BrokenOverGood", good, MissingAssembly);
        GoodOverBroken = Script(module, "GoodOverBroken", broken, typeof(Camera).AssemblyQualifiedName!);
    }

    private static Type Script(ModuleBuilder module, string name, Type parent, string requiredType)
    {
        var type = module.DefineType(name, TypeAttributes.Public | TypeAttributes.Class, parent);
        type.DefineDefaultConstructor(MethodAttributes.Public);
        type.SetCustomAttribute(typeof(RequireComponent).GetConstructor([typeof(Type)])!, Blob(requiredType));
        return type.CreateType();
    }

    private static byte[] Blob(string typeName)
    {
        var text = Encoding.UTF8.GetBytes(typeName);
        return [1, 0, .. Length(text.Length), .. text, 0, 0];
    }

    private static byte[] Length(int length) =>
        length < 0x80 ? [(byte)length] : [(byte)(0x80 | (length >> 8)), (byte)length];
}
