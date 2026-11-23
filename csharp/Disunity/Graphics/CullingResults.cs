using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// What culling found: a handle to the culling result <see cref="ScriptableRenderContext.Cull"/> recorded in the world, which
/// later draws name, and the lights that <c>Cull</c> found visible when it ran. <see cref="visibleLights"/> lists the
/// directional lights first and then the point and spot lights whose range sphere meets the view volume, each group in the
/// order the lights were created. The engine does not cull offscreen vertex lights or reflection probes, so those arrays are
/// empty, and it keeps no per-object light lists: the index counts are 0 and filling an index buffer writes nothing. The light
/// index map is an identity map that <see cref="SetLightIndexMap"/> may replace with a map of the same length; it changes
/// nothing drawn. The shadow queries find the culled renderers that cast shadows (see <see cref="GetShadowCasterBounds"/>) and fit the light's shadow maps in the native shadow module. A light index outside the
/// visible lights throws <see cref="ArgumentOutOfRangeException"/>. The default value is no result and has no lights.
/// </summary>
public struct CullingResults : IEquatable<CullingResults>
{
    /// <summary>The id the world's culling record has; 0 for no result.</summary>
    internal ulong Handle;

    /// <summary>What the live batch renderer groups draw for the view, asked when the culling ran; null for the default value.</summary>
    internal BatchCull? Batches;

    /// <summary>The lights found when the culling ran; null for the default value.</summary>
    internal VisibleLightSet? Lights;

    /// <summary>The casters the shadow queries ask about, or null for the default value.</summary>
    internal ShadowCasterSet? Casters;

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

    /// <summary>The world-space box around the culled renderers whose shadow casting mode is not off and that the light can
    /// affect, or false with default bounds when there are none. A directional light is affected by every such renderer, and a
    /// point or spot light by those within its range. The renderers are found at the first shadow query on a result, and the
    /// casters outside the camera's view count as long as the culling mask draws their layer.</summary>
    public readonly bool GetShadowCasterBounds(int lightIndex, out Bounds outBounds)
    {
        RequireLight(lightIndex, nameof(lightIndex));
        var answer = Query(lightIndex, ShadowQueryKind.Bounds, default);
        var result = answer.Result;
        outBounds = default;
        if (!answer.Found) return false;
        outBounds.SetMinMax(new Vector3(result.min_x, result.min_y, result.min_z), new Vector3(result.max_x, result.max_y, result.max_z));
        return true;
    }

    /// <summary>The view and projection of a spot light's shadow map and the planes its casters must not be outside of. False,
    /// with identity matrices and empty split data, when no renderer in the light's range casts a shadow.</summary>
    public readonly bool ComputeSpotShadowMatricesAndCullingPrimitives(int activeLightIndex, out Matrix4x4 viewMatrix,
        out Matrix4x4 projMatrix, out ShadowSplitData shadowSplitData)
    {
        RequireLight(activeLightIndex, nameof(activeLightIndex));
        return Split(Query(activeLightIndex, ShadowQueryKind.Spot, default), out viewMatrix, out projMatrix, out shadowSplitData);
    }

    /// <summary>The view and projection of one face of a point light's shadow map, with <paramref name="fovBias"/> degrees added
    /// to the quarter turn each face looks through. False, as for a spot light, when there is no caster in range. A face outside
    /// the cube throws <see cref="ArgumentException"/>.</summary>
    public readonly bool ComputePointShadowMatricesAndCullingPrimitives(int activeLightIndex, CubemapFace cubemapFace, float fovBias,
        out Matrix4x4 viewMatrix, out Matrix4x4 projMatrix, out ShadowSplitData shadowSplitData)
    {
        RequireLight(activeLightIndex, nameof(activeLightIndex));
        var query = new ShadowQueryArguments { Face = (uint)cubemapFace, FovBias = fovBias };
        return Split(Query(activeLightIndex, ShadowQueryKind.Point, query), out viewMatrix, out projMatrix, out shadowSplitData);
    }

    /// <summary>The view and projection of one cascade of a directional light's shadow map, the culling sphere around the camera's
    /// slice of view for that cascade and the planes its casters must not be outside of. <paramref name="splitRatio"/> holds the far
    /// edge of each of the first three cascades as a fraction of the shadow distance; the last cascade ends at the shadow distance.
    /// The map is fitted to the slice, with its centre snapped to whole texels of <paramref name="shadowResolution"/>, and reaches
    /// toward the light over the casters that shadow it, plus <paramref name="shadowNearPlaneOffset"/>. False, with identity
    /// matrices and empty split data, when no renderer casts a shadow. A split outside <paramref name="splitCount"/> (1 to 4), ratios
    /// that are not above 0, ascending and at most 1, or a resolution below 4 throw <see cref="ArgumentException"/>.</summary>
    public readonly bool ComputeDirectionalShadowMatricesAndCullingPrimitives(int activeLightIndex, int splitIndex, int splitCount,
        Vector3 splitRatio, int shadowResolution, float shadowNearPlaneOffset, out Matrix4x4 viewMatrix, out Matrix4x4 projMatrix,
        out ShadowSplitData shadowSplitData)
    {
        RequireLight(activeLightIndex, nameof(activeLightIndex));
        var query = new ShadowQueryArguments
        {
            SplitIndex = (uint)splitIndex, SplitCount = (uint)splitCount, Ratio = splitRatio, Resolution = (uint)shadowResolution,
            NearPlaneOffset = shadowNearPlaneOffset,
        };
        return Split(Query(activeLightIndex, ShadowQueryKind.Directional, query), out viewMatrix, out projMatrix, out shadowSplitData);
    }

    private readonly ShadowCasterSet.Answer Query(int lightIndex, ShadowQueryKind kind, ShadowQueryArguments arguments)
    {
        var casters = Casters ?? throw new InvalidOperationException("the culling result holds no shadow casters");
        var light = Lights!.Lights[lightIndex];
        return casters.Ask(light, arguments.Apply(casters.RequestFor(light, (uint)kind)));
    }

    /// <summary>Fills the matrices and split data from an answer, or gives identity matrices and empty split data when no caster cast.</summary>
    private static bool Split(ShadowCasterSet.Answer answer, out Matrix4x4 view, out Matrix4x4 projection, out ShadowSplitData split)
    {
        (view, projection, split) = (Matrix4x4.identity, Matrix4x4.identity, default);
        if (!answer.Found) return false;
        (view, projection) = (answer.Matrix(0), answer.Matrix(1));
        var result = answer.Result;
        split.cullingMatrix = answer.Matrix(2);
        split.cullingSphere = new Vector4(result.sphere_x, result.sphere_y, result.sphere_z, result.sphere_radius);
        split.cullingPlaneCount = (int)result.plane_count;
        for (var index = 0; index < split.cullingPlaneCount; index++)
        {
            var at = 48 + index * 4;
            split.SetCullingPlane(index, new Plane(new Vector3(answer.Output[at], answer.Output[at + 1], answer.Output[at + 2]), answer.Output[at + 3]));
        }
        return true;
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
