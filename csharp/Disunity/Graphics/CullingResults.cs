using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// What culling found: a handle to the culling result <see cref="ScriptableRenderContext.Cull"/> recorded in the world, which
/// later draws name. The engine has no light or probe culling, so the three arrays are empty and every method that needs
/// them throws <see cref="NotSupportedException"/> naming the member. The default value is no result.
/// </summary>
public struct CullingResults : IEquatable<CullingResults>
{
    /// <summary>The id the world's culling record has; 0 for no result.</summary>
    internal ulong Handle;

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
        new($"CullingResults.{member} needs light culling, and the engine does not cull lights or probes yet");

    public readonly bool Equals(CullingResults other) => Handle == other.Handle;
    public override readonly bool Equals(object? obj) => obj is CullingResults other && Equals(other);
    public override readonly int GetHashCode() => Handle.GetHashCode();
    public static bool operator ==(CullingResults left, CullingResults right) => left.Equals(right);
    public static bool operator !=(CullingResults left, CullingResults right) => !left.Equals(right);
}
