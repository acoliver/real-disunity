using System.Reflection;
using UnityEngine;

/// <summary>A compiled game binds the ComputeShader members it calls when a method that uses them is first compiled, so
/// the members the terrain read-back calls must exist with the signatures the game was built against. The engine builds
/// no compute shaders, so a script's field binds null and the game's own handler logs the failed call.</summary>
internal static class ComputeShaderTests
{
    public static void Run()
    {
        var bindings = new (string Name, Type[] Parameters)[]
        {
            ("GetKernelThreadGroupSizes", new[] { typeof(int), typeof(uint).MakeByRefType(), typeof(uint).MakeByRefType(), typeof(uint).MakeByRefType() }),
            ("SetBuffer", new[] { typeof(int), typeof(string), typeof(ComputeBuffer) }),
            ("SetInt", new[] { typeof(string), typeof(int) }),
            ("SetTextureFromGlobal", new[] { typeof(int), typeof(string), typeof(string) }),
            ("Dispatch", new[] { typeof(int), typeof(int), typeof(int), typeof(int) }),
        };
        foreach (var (name, parameters) in bindings)
        {
            var method = typeof(ComputeShader).GetMethod(name, BindingFlags.Public | BindingFlags.Instance, null, parameters, null);
            Expect.That(method is not null && method.ReturnType == typeof(void), $"ComputeShader.{name} binds with the game's signature");
        }
        Console.WriteLine("PASS: ComputeShader declares the members the terrain read-back calls, so a method that calls them compiles");
    }
}
