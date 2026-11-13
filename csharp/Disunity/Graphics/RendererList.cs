using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// A list of renderers to draw, made by <see cref="ScriptableRenderContext.CreateRendererList"/> or
/// <see cref="ScriptableRenderContext.CreateShadowRendererList"/> and drawn by <see cref="CommandBuffer.DrawRendererList"/>.
/// It holds the id of the list the world recorded, which names a list of the frame that made it. The default value is the null
/// list, which is never valid. Lists are equal when their ids are equal.
/// </summary>
public struct RendererList : IEquatable<RendererList>
{
    /// <summary>The id of the world's list record; 0 for no list.</summary>
    internal ulong Id;

    /// <summary>The camera's screen-space overlay UI, which the engine's UI presentation draws after the cameras, so drawing
    /// the list records nothing.</summary>
    internal bool UiOverlay;

    public static readonly RendererList nullRendererList = default;

    public readonly bool isValid => Id != 0 || UiOverlay;

    public readonly bool Equals(RendererList other) => Id == other.Id && UiOverlay == other.UiOverlay;
    public override readonly bool Equals(object? obj) => obj is RendererList other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(Id, UiOverlay);
    public static bool operator ==(RendererList left, RendererList right) => left.Equals(right);
    public static bool operator !=(RendererList left, RendererList right) => !left.Equals(right);
}

/// <summary>What a renderer list is made from: the culling results, the shader passes to draw (<c>drawSettings</c> names them, with
/// the sorting criteria, per-object data and override material) and the filter on queue, layers and rendering layers. The
/// pass tag fields <c>tagName</c>, <c>isPassTagName</c>, <c>tagValues</c> and <c>stateBlocks</c> are stored, and the engine
/// reads none of them; the last two are nullable arrays, null unless the pipeline sets them, because the pipelines read and
/// write them as such. It is valid once its culling results hold a culling result.</summary>
public struct RendererListParams
{
    public CullingResults cullingResults;
    public DrawingSettings drawSettings;
    public FilteringSettings filteringSettings;
    public ShaderTagId tagName;
    public bool isPassTagName;
    public NativeArray<ShaderTagId>? tagValues;
    public NativeArray<RenderStateBlock>? stateBlocks;

    public RendererListParams(CullingResults cullingResults, DrawingSettings drawSettings, FilteringSettings filteringSettings)
    {
        this.cullingResults = cullingResults;
        this.drawSettings = drawSettings;
        this.filteringSettings = filteringSettings;
        tagName = ShaderTagId.none;
        isPassTagName = false;
        tagValues = null;
        stateBlocks = null;
    }

    public readonly bool isValid => cullingResults.Handle != 0;
}
