using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>
    /// Makes a compute shader the way player loading does from a plan: its name, the serialized file and object its kernels are
    /// in, and its kernels in index order, each with the thread group size the data states or null when none is stated (the
    /// host then states it through its GPU flush). Unlike <see cref="RegisterShader"/>, every call makes a new compute shader.
    /// </summary>
    public static ComputeShader RegisterComputeShader(string name, string file, long pathId, IReadOnlyList<(string Name, uint[]? Size)> kernels)
    {
        ArgumentNullException.ThrowIfNull(name);
        ArgumentNullException.ThrowIfNull(file);
        ArgumentNullException.ThrowIfNull(kernels);
        return ComputeShader.FromPlan(name, file, pathId, kernels);
    }
}
