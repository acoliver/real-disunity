namespace UnityEngine;

/// <summary>
/// A compute shader asset. The engine builds no compute shaders from player data, so a script field of this type binds
/// null, and the type declares none of the members a script calls to dispatch one: a script that calls one fails
/// to bind the member instead of running a stand-in. It exists so that script types with such fields load.
/// </summary>
public sealed class ComputeShader : Object
{
    private ComputeShader() { }
}
