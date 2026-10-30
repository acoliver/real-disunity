using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// What culling found. The engine does not cull, so the three arrays are the empty default and every method that
/// needs a culling result throws <see cref="NotSupportedException"/> naming the member.
/// </summary>
public struct CullingResults : IEquatable<CullingResults>
{
    public readonly NativeArray<VisibleLight> visibleLights => default;
    public readonly NativeArray<VisibleLight> visibleOffscreenVertexLights => default;
    public readonly NativeArray<VisibleReflectionProbe> visibleReflectionProbes => default;

    public readonly int lightIndexCount => throw Unsupported("lightIndexCount");
    public readonly int reflectionProbeIndexCount => throw Unsupported("reflectionProbeIndexCount");
    public readonly int lightAndReflectionProbeIndexCount => throw Unsupported("lightAndReflectionProbeIndexCount");

    public readonly NativeArray<int> GetLightIndexMap(Allocator allocator) => throw Unsupported("GetLightIndexMap");
    public readonly void SetLightIndexMap(NativeArray<int> lightIndexMap) => throw Unsupported("SetLightIndexMap");
    public readonly NativeArray<int> GetReflectionProbeIndexMap(Allocator allocator) => throw Unsupported("GetReflectionProbeIndexMap");
    public readonly void SetReflectionProbeIndexMap(NativeArray<int> lightIndexMap) => throw Unsupported("SetReflectionProbeIndexMap");

    public readonly bool GetShadowCasterBounds(int lightIndex, out Bounds outBounds) => throw Unsupported("GetShadowCasterBounds");

    public readonly bool ComputeSpotShadowMatricesAndCullingPrimitives(int activeLightIndex, out Matrix4x4 viewMatrix,
        out Matrix4x4 projMatrix, out ShadowSplitData shadowSplitData) =>
        throw Unsupported("ComputeSpotShadowMatricesAndCullingPrimitives");

    public readonly bool ComputePointShadowMatricesAndCullingPrimitives(int activeLightIndex, CubemapFace cubemapFace, float fovBias,
        out Matrix4x4 viewMatrix, out Matrix4x4 projMatrix, out ShadowSplitData shadowSplitData) =>
        throw Unsupported("ComputePointShadowMatricesAndCullingPrimitives");

    public readonly bool ComputeDirectionalShadowMatricesAndCullingPrimitives(int activeLightIndex, int splitIndex, int splitCount,
        Vector3 splitRatio, int shadowResolution, float shadowNearPlaneOffset, out Matrix4x4 viewMatrix, out Matrix4x4 projMatrix,
        out ShadowSplitData shadowSplitData) =>
        throw Unsupported("ComputeDirectionalShadowMatricesAndCullingPrimitives");

    private static NotSupportedException Unsupported(string member) =>
        new($"CullingResults.{member} needs culling, and the engine does not cull yet");

    // No culling result exists, so every value is the empty one and equal to every other.
    public readonly bool Equals(CullingResults other) => true;
    public override readonly bool Equals(object? obj) => obj is CullingResults;
    public override readonly int GetHashCode() => 0;
    public static bool operator ==(CullingResults left, CullingResults right) => left.Equals(right);
    public static bool operator !=(CullingResults left, CullingResults right) => !left.Equals(right);
}
