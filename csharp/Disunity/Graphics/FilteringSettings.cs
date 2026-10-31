namespace UnityEngine.Rendering;

/// <summary>The named render queue values a material's queue is ordered by.</summary>
public enum RenderQueue
{
    Background = 1000,
    Geometry = 2000,
    AlphaTest = 2450,
    GeometryLast = 2500,
    Transparent = 3000,
    Overlay = 4000,
}

/// <summary>A closed range of sorting layer values. Plain data; equality compares both bounds.</summary>
public struct SortingLayerRange : IEquatable<SortingLayerRange>
{
    public short lowerBound;
    public short upperBound;

    public SortingLayerRange(short lowerBound, short upperBound)
    {
        this.lowerBound = lowerBound;
        this.upperBound = upperBound;
    }

    public static SortingLayerRange all => new(short.MinValue, short.MaxValue);

    public readonly bool Equals(SortingLayerRange other) => lowerBound == other.lowerBound && upperBound == other.upperBound;
    public override readonly bool Equals(object? obj) => obj is SortingLayerRange other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(lowerBound, upperBound);
    public static bool operator ==(SortingLayerRange left, SortingLayerRange right) => left.Equals(right);
    public static bool operator !=(SortingLayerRange left, SortingLayerRange right) => !left.Equals(right);
}

/// <summary>
/// Which renderers a draw call takes: by render queue range, layer mask, rendering layer mask and sorting layer
/// range. Plain data; the engine does not cull or draw with it yet. A null render queue range means all queues.
/// </summary>
public struct FilteringSettings : IEquatable<FilteringSettings>
{
    public RenderQueueRange renderQueueRange { get; set; }
    public int layerMask { get; set; }
    public uint renderingLayerMask { get; set; }
    public uint batchLayerMask { get; set; }
    public SortingLayerRange sortingLayerRange { get; set; }
    public bool excludeMotionVectorObjects { get; set; }

    public FilteringSettings(RenderQueueRange? renderQueueRange = null, int layerMask = -1, uint renderingLayerMask = uint.MaxValue, int excludeMotionVectorObjects = 0)
    {
        this.renderQueueRange = renderQueueRange ?? RenderQueueRange.all;
        this.layerMask = layerMask;
        this.renderingLayerMask = renderingLayerMask;
        batchLayerMask = uint.MaxValue;
        sortingLayerRange = SortingLayerRange.all;
        this.excludeMotionVectorObjects = excludeMotionVectorObjects != 0;
    }

    /// <summary>All queues, all layers, all rendering layers and all sorting layers.</summary>
    public static FilteringSettings defaultValue => new(RenderQueueRange.all, -1, uint.MaxValue, 0);

    public readonly bool Equals(FilteringSettings other) =>
        renderQueueRange == other.renderQueueRange && layerMask == other.layerMask
        && renderingLayerMask == other.renderingLayerMask && batchLayerMask == other.batchLayerMask
        && sortingLayerRange == other.sortingLayerRange && excludeMotionVectorObjects == other.excludeMotionVectorObjects;

    public override readonly bool Equals(object? obj) => obj is FilteringSettings other && Equals(other);

    public override readonly int GetHashCode() =>
        HashCode.Combine(renderQueueRange, layerMask, renderingLayerMask, batchLayerMask, sortingLayerRange, excludeMotionVectorObjects);

    public static bool operator ==(FilteringSettings left, FilteringSettings right) => left.Equals(right);
    public static bool operator !=(FilteringSettings left, FilteringSettings right) => !left.Equals(right);
}
