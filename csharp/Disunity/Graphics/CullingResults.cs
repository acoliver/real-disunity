using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// What culling found: a handle to the culling result <see cref="ScriptableRenderContext.Cull"/> recorded in the world, which
/// later draws name, and the lights that <c>Cull</c> found visible when it ran. <see cref="visibleLights"/> lists the
/// directional lights first and then the point and spot lights whose range sphere meets the view volume, each group in the
/// order the lights were created. The engine does not cull offscreen vertex lights or reflection probes, so those arrays are
/// empty, and it keeps no per-object light lists: the index counts are 0 and filling an index buffer writes nothing. The light
/// index map is an identity map that <see cref="SetLightIndexMap"/> may replace with a map of the same length; it changes
/// nothing drawn. The engine does not cull shadow casters, so every shadow query reports none in range. A light index outside the
/// visible lights throws <see cref="ArgumentOutOfRangeException"/>. The default value is no result and has no lights.
/// </summary>
public struct CullingResults : IEquatable<CullingResults>
{
    /// <summary>The id the world's culling record has; 0 for no result.</summary>
    internal ulong Handle;

    /// <summary>The lights found when the culling ran; null for the default value.</summary>
    internal VisibleLightSet? Lights;

    /// <summary>The visible lights. The array stays valid while this result, or a copy of it, is reachable; do not dispose it.</summary>
    public readonly NativeArray<VisibleLight> visibleLights => Lights?.AsNativeArray() ?? default;
    public readonly NativeArray<VisibleLight> visibleOffscreenVertexLights => default;
    public readonly NativeArray<VisibleReflectionProbe> visibleReflectionProbes => default;

    public readonly int lightIndexCount => 0;
    public readonly int reflectionProbeIndexCount => 0;
    public readonly int lightAndReflectionProbeIndexCount => 0;

    /// <summary>Writes each object's light and probe indices into the buffer. There are none, so the buffer is left as it is.</summary>
    public readonly void FillLightAndReflectionProbeIndices(ComputeBuffer computeBuffer) => ArgumentNullException.ThrowIfNull(computeBuffer);

    /// <summary>Writes each object's light and probe indices into the buffer. There are none, so the buffer is left as it is.</summary>
    public readonly void FillLightAndReflectionProbeIndices(GraphicsBuffer buffer) => ArgumentNullException.ThrowIfNull(buffer);

    private readonly int LightCount => Lights?.Count ?? 0;

    /// <summary>The map from each visible light to the light index a pipeline uses: one entry per visible light (there are no
    /// offscreen vertex lights), the identity until <see cref="SetLightIndexMap"/> replaces it. The caller owns the new array.</summary>
    public readonly NativeArray<int> GetLightIndexMap(Allocator allocator) =>
        Lights?.GetIndexMap(allocator) ?? new NativeArray<int>(0, allocator);

    /// <summary>Stores a copy of the map. Its length must equal the number of visible lights, otherwise <see cref="ArgumentException"/>.
    /// With no per-object light lists the map changes nothing drawn.</summary>
    public readonly void SetLightIndexMap(NativeArray<int> lightIndexMap)
    {
        RequireLength(lightIndexMap, LightCount, nameof(lightIndexMap), "light");
        Lights?.SetIndexMap(lightIndexMap);
    }

    /// <summary>An empty map: the engine has no reflection probes.</summary>
    public readonly NativeArray<int> GetReflectionProbeIndexMap(Allocator allocator) => new(0, allocator);

    /// <summary>Checks that the map is empty, as there are no reflection probes, and otherwise throws <see cref="ArgumentException"/>.</summary>
    public readonly void SetReflectionProbeIndexMap(NativeArray<int> lightIndexMap) =>
        RequireLength(lightIndexMap, 0, nameof(lightIndexMap), "reflection probe");

    /// <summary>False with default bounds: the engine does not cull shadow casters, so none is in range of any light, and a pipeline
    /// skips the light's shadows.</summary>
    public readonly bool GetShadowCasterBounds(int lightIndex, out Bounds outBounds)
    {
        RequireLight(lightIndex, nameof(lightIndex));
        outBounds = default;
        return false;
    }

    public readonly bool ComputeSpotShadowMatricesAndCullingPrimitives(int activeLightIndex, out Matrix4x4 viewMatrix,
        out Matrix4x4 projMatrix, out ShadowSplitData shadowSplitData)
    {
        RequireLight(activeLightIndex, nameof(activeLightIndex));
        return NoShadowCasters(out viewMatrix, out projMatrix, out shadowSplitData);
    }

    public readonly bool ComputePointShadowMatricesAndCullingPrimitives(int activeLightIndex, CubemapFace cubemapFace, float fovBias,
        out Matrix4x4 viewMatrix, out Matrix4x4 projMatrix, out ShadowSplitData shadowSplitData)
    {
        RequireLight(activeLightIndex, nameof(activeLightIndex));
        return NoShadowCasters(out viewMatrix, out projMatrix, out shadowSplitData);
    }

    public readonly bool ComputeDirectionalShadowMatricesAndCullingPrimitives(int activeLightIndex, int splitIndex, int splitCount,
        Vector3 splitRatio, int shadowResolution, float shadowNearPlaneOffset, out Matrix4x4 viewMatrix, out Matrix4x4 projMatrix,
        out ShadowSplitData shadowSplitData)
    {
        RequireLight(activeLightIndex, nameof(activeLightIndex));
        return NoShadowCasters(out viewMatrix, out projMatrix, out shadowSplitData);
    }

    /// <summary>The answer for a light with no shadow casters in range: false, identity matrices and default split data.</summary>
    private static bool NoShadowCasters(out Matrix4x4 viewMatrix, out Matrix4x4 projMatrix, out ShadowSplitData shadowSplitData)
    {
        (viewMatrix, projMatrix, shadowSplitData) = (Matrix4x4.identity, Matrix4x4.identity, default);
        return false;
    }

    private readonly void RequireLight(int index, string parameter)
    {
        if ((uint)index >= (uint)LightCount)
            throw new ArgumentOutOfRangeException(parameter, index, $"CullingResults has {LightCount} visible lights");
    }

    private static void RequireLength(NativeArray<int> map, int expected, string parameter, string kind)
    {
        if (map.Length != expected)
            throw new ArgumentException($"the {kind} index map has {map.Length} entries and CullingResults has {expected} {kind}s", parameter);
    }

    public readonly bool Equals(CullingResults other) => Handle == other.Handle;
    public override readonly bool Equals(object? obj) => obj is CullingResults other && Equals(other);
    public override readonly int GetHashCode() => Handle.GetHashCode();
    public static bool operator ==(CullingResults left, CullingResults right) => left.Equals(right);
    public static bool operator !=(CullingResults left, CullingResults right) => !left.Equals(right);
}
