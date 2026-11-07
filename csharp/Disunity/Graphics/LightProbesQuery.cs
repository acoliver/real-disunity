using Unity.Collections;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A query over the light probes of a scene. The engine bakes and stores no light probes, so constructing and
/// disposing a query does nothing and the queries throw <see cref="NotSupportedException"/>.
/// </summary>
public struct LightProbesQuery : IDisposable
{
    internal readonly Allocator Allocator;

    public LightProbesQuery(Allocator allocator) => Allocator = allocator;

    private static NotSupportedException Unsupported(string member) => new($"LightProbesQuery.{member} is not supported: the engine has no light probes");

    public void Dispose() { }

    public void CalculateInterpolatedLightAndOcclusionProbe(Vector3 position, out SphericalHarmonicsL2 lightProbe, out Vector4 occlusionProbe) =>
        throw Unsupported(nameof(CalculateInterpolatedLightAndOcclusionProbe));

    public void CalculateInterpolatedLightAndOcclusionProbes(NativeArray<Vector3> positions, NativeArray<SphericalHarmonicsL2> lightProbes,
        NativeArray<Vector4> occlusionProbes) => throw Unsupported(nameof(CalculateInterpolatedLightAndOcclusionProbes));
}
