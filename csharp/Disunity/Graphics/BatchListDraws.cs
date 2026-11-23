namespace UnityEngine.Rendering;

/// <summary>
/// The batch draws of a renderer list, recorded when the list is drawn. A list takes a draw of its culling's view when
/// its filter accepts it:
/// <list type="bullet">
/// <item>A list made by <c>CreateRendererList</c> accepts a draw whose material's render queue is inside the list's queue range
/// (bounds included), whose draw range shares a bit with the list's rendering layer mask, and whose draw range does not cast
/// shadows only.</item>
/// <item>A list made by <c>CreateShadowRendererList</c> accepts a draw whose draw range casts shadows (any mode but off). The
/// light's rendering layer mask is not known here, so the layer test of a shadow list does not apply to batches.</item>
/// </list>
/// Every list of a camera asks the same draws, so a draw is recorded once for each list that accepts it. The draw's pass tags are
/// the list's: the pass names of the drawing settings, or <c>ShadowCaster</c> for a shadow list.
/// </summary>
internal sealed class BatchListDraws
{
    internal const string ShadowCasterTag = "ShadowCaster";

    private readonly IReadOnlyList<BatchDraw> _draws;
    private readonly string[] _tags;
    private readonly Func<BatchDraw, bool> _accepts;

    private BatchListDraws(IReadOnlyList<BatchDraw> draws, string[] tags, Func<BatchDraw, bool> accepts) => (_draws, _tags, _accepts) = (draws, tags, accepts);

    /// <summary>The draws of a drawing list: the pass names it draws, the queue range and the rendering layer mask it filters by.</summary>
    internal static BatchListDraws ForDrawing(BatchCull cull, string[] tags, RenderQueueRange queues, uint renderingLayerMask) =>
        new(cull.Draws, tags, draw => InQueues(draw, queues) && (draw.Filter.renderingLayerMask & renderingLayerMask) != 0
            && draw.Filter.shadowCastingMode != ShadowCastingMode.ShadowsOnly);

    /// <summary>The draws of a shadow list, which come from the culling for the shadow split.</summary>
    internal static BatchListDraws ForShadows(BatchCull cull) =>
        new(cull.Draws, [ShadowCasterTag], draw => draw.Filter.shadowCastingMode != ShadowCastingMode.Off);

    private static bool InQueues(BatchDraw draw, RenderQueueRange queues)
    {
        if (!draw.Material) return false;
        var queue = draw.Material.renderQueue;
        return queue >= queues.lowerBound && queue <= queues.upperBound;
    }

    /// <summary>Records each accepted draw in the order the callbacks wrote them. Called when the buffer that draws the list runs.</summary>
    internal void Record()
    {
        foreach (var draw in _draws)
        {
            if (_accepts(draw)) WorldGraphics.RecordDrawBatch(draw, _tags);
        }
    }
}
