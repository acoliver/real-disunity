namespace UnityEngine;

/// <summary>
/// A compute shader asset. The engine builds no compute shaders from player data, so a script field of this type binds
/// null and no instance exists. The members a compiled game calls to read a kernel and dispatch it are declared so that
/// a method that calls them compiles; each throws <see cref="NotSupportedException"/> until the native draw surface
/// can run compute work (#171). Members no game run has needed are not declared.
/// </summary>
public sealed class ComputeShader : Object
{
    private ComputeShader() { }

    private static NotSupportedException Unsupported(string member) =>
        new($"ComputeShader.{member} is not supported yet: the engine builds no compute shaders");

    public void GetKernelThreadGroupSizes(int kernelIndex, out uint x, out uint y, out uint z) => throw Unsupported(nameof(GetKernelThreadGroupSizes));
    public void SetBuffer(int kernelIndex, string name, ComputeBuffer buffer) => throw Unsupported(nameof(SetBuffer));
    public void SetInt(string name, int val) => throw Unsupported(nameof(SetInt));
    public void SetTextureFromGlobal(int kernelIndex, string name, string globalTextureName) => throw Unsupported(nameof(SetTextureFromGlobal));
    public void Dispatch(int kernelIndex, int threadGroupsX, int threadGroupsY, int threadGroupsZ) => throw Unsupported(nameof(Dispatch));
}
