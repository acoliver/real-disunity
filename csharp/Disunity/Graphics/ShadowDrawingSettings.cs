namespace UnityEngine.Rendering;

/// <summary>Which objects a shadow caster draw includes.</summary>
public enum ShadowObjectsFilter
{
    AllObjects = 0,
    DynamicOnly = 1,
    StaticOnly = 2,
}

/// <summary>
/// How a shadow caster draw is set up: the culling results and the visible light it draws for, the split's culling volume,
/// which objects it includes, whether it tests rendering layers and the projection. A new value names the given culling
/// results and light, has an empty split, includes all objects, does not test rendering layers and uses a perspective
/// projection unless one is given. Plain data; the engine does not draw shadow casters yet.
/// </summary>
public struct ShadowDrawingSettings : IEquatable<ShadowDrawingSettings>
{
    public CullingResults cullingResults { get; set; }
    public int lightIndex { get; set; }
    public bool useRenderingLayerMaskTest { get; set; }
    public ShadowSplitData splitData { get; set; }
    public ShadowObjectsFilter objectsFilter { get; set; }
    public BatchCullingProjectionType projectionType { get; set; }

    public ShadowDrawingSettings(CullingResults cullingResults, int lightIndex)
        : this(cullingResults, lightIndex, BatchCullingProjectionType.Perspective)
    {
    }

    public ShadowDrawingSettings(CullingResults cullingResults, int lightIndex, BatchCullingProjectionType projectionType)
    {
        this.cullingResults = cullingResults;
        this.lightIndex = lightIndex;
        this.projectionType = projectionType;
        useRenderingLayerMaskTest = false;
        splitData = default;
        objectsFilter = ShadowObjectsFilter.AllObjects;
    }

    public readonly bool Equals(ShadowDrawingSettings other) =>
        cullingResults == other.cullingResults && lightIndex == other.lightIndex
        && useRenderingLayerMaskTest == other.useRenderingLayerMaskTest && splitData == other.splitData
        && objectsFilter == other.objectsFilter && projectionType == other.projectionType;

    public override readonly bool Equals(object? obj) => obj is ShadowDrawingSettings other && Equals(other);

    public override readonly int GetHashCode() =>
        HashCode.Combine(cullingResults, lightIndex, useRenderingLayerMaskTest, splitData, objectsFilter, projectionType);

    public static bool operator ==(ShadowDrawingSettings left, ShadowDrawingSettings right) => left.Equals(right);
    public static bool operator !=(ShadowDrawingSettings left, ShadowDrawingSettings right) => !left.Equals(right);
}
