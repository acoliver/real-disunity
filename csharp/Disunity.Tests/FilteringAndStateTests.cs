using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The data a scriptable render pipeline builds a draw from: filtering, sorting, drawing and render states.</summary>
internal static class FilteringAndStateTests
{
    public static void Run()
    {
        Filtering();
        StateStructs();
        Stencil();
        RenderStateBlocks();
        Console.WriteLine("PASS: FilteringSettings, RenderQueueRange, BlendState, DepthState, RasterState, StencilState and RenderStateBlock hold their data");
        DisunityHost.LoadWorld();
        try { Drawing(); }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: SortingSettings and DrawingSettings hold their data");
    }

    private static void Filtering()
    {
        Expect.Equal(0, RenderQueueRange.all.lowerBound, "all starts at 0");
        Expect.Equal(5000, RenderQueueRange.all.upperBound, "all ends at 5000");
        Expect.Equal(2500, RenderQueueRange.opaque.upperBound, "opaque ends at 2500");
        Expect.Equal(2501, RenderQueueRange.transparent.lowerBound, "transparent starts after opaque");
        Expect.Equal(5000, RenderQueueRange.transparent.upperBound, "transparent ends at 5000");
        Expect.That(RenderQueueRange.opaque != RenderQueueRange.transparent && RenderQueueRange.all == RenderQueueRange.all, "range equality");
        Expect.Equal(2000, (int)RenderQueue.Geometry, "Geometry queue");

        var defaults = FilteringSettings.defaultValue;
        Expect.Equal(RenderQueueRange.all, defaults.renderQueueRange, "default range");
        Expect.Equal(-1, defaults.layerMask, "default layer mask");
        Expect.Equal(uint.MaxValue, defaults.renderingLayerMask, "default rendering layer mask");
        Expect.That(!defaults.excludeMotionVectorObjects, "default includes motion vector objects");
        Expect.Equal(SortingLayerRange.all, defaults.sortingLayerRange, "default sorting layers");
        Expect.Equal(defaults, new FilteringSettings(null, -1, uint.MaxValue, 0), "null range and the default arguments give the default");

        var opaque = new FilteringSettings(RenderQueueRange.opaque, 0x5, 0x3, 1);
        Expect.Equal(RenderQueueRange.opaque, opaque.renderQueueRange, "range");
        Expect.Equal(5, opaque.layerMask, "layer mask");
        Expect.Equal(3u, opaque.renderingLayerMask, "rendering layer mask");
        Expect.That(opaque.excludeMotionVectorObjects, "a nonzero argument excludes motion vector objects");
        Expect.That(opaque != defaults && opaque == new FilteringSettings(RenderQueueRange.opaque, 0x5, 0x3, 1), "equality");
        var changed = opaque;
        changed.layerMask = 6;
        Expect.That(changed != opaque, "layer mask takes part in equality");
        changed = opaque;
        changed.renderingLayerMask = 4;
        Expect.That(changed != opaque, "rendering layer mask takes part in equality");
        changed = opaque;
        changed.sortingLayerRange = new SortingLayerRange(1, 2);
        Expect.That(changed != opaque, "sorting layer range takes part in equality");
        Expect.Equal(opaque.GetHashCode(), new FilteringSettings(RenderQueueRange.opaque, 0x5, 0x3, 1).GetHashCode(), "equal values hash equally");
        Expect.Equal(RenderQueueRange.all, new FilteringSettings(null, 1).renderQueueRange, "a null range means all queues");
    }

    private static void StateStructs()
    {
        var target = RenderTargetBlendState.defaultValue;
        Expect.Equal(ColorWriteMask.All, target.writeMask, "target writes all channels");
        Expect.Equal(BlendMode.One, target.sourceColorBlendMode, "source color one");
        Expect.Equal(BlendMode.Zero, target.destinationColorBlendMode, "destination color zero");
        Expect.Equal(BlendMode.One, target.sourceAlphaBlendMode, "source alpha one");
        Expect.Equal(BlendMode.Zero, target.destinationAlphaBlendMode, "destination alpha zero");
        Expect.Equal(BlendOp.Add, target.colorBlendOperation, "color add");
        Expect.Equal(BlendOp.Add, target.alphaBlendOperation, "alpha add");
        var alpha = new RenderTargetBlendState(ColorWriteMask.Red, BlendMode.SrcAlpha, BlendMode.OneMinusSrcAlpha, BlendMode.One, BlendMode.One, BlendOp.Max, BlendOp.Min);
        Expect.That(alpha.writeMask == ColorWriteMask.Red && alpha.destinationColorBlendMode == BlendMode.OneMinusSrcAlpha
            && alpha.colorBlendOperation == BlendOp.Max && alpha.alphaBlendOperation == BlendOp.Min, "target blend arguments land in their properties");
        Expect.That(alpha != target && target == RenderTargetBlendState.defaultValue, "target blend equality");

        var blend = BlendState.defaultValue;
        Expect.That(!blend.separateMRTBlendStates && !blend.alphaToMask, "blend flags off");
        Expect.Equal(target, blend.blendState7, "every target holds the default blend");
        var custom = new BlendState(true, true) { blendState3 = alpha };
        Expect.That(custom.separateMRTBlendStates && custom.alphaToMask && custom.blendState3 == alpha, "blend state arguments and targets");
        Expect.That(custom != blend && blend == BlendState.defaultValue, "blend state equality");
        var last = blend;
        last.blendState7 = alpha;
        Expect.That(last != blend, "the eighth target takes part in equality");

        var depth = DepthState.defaultValue;
        Expect.That(depth.writeEnabled && depth.compareFunction == CompareFunction.Less, "depth defaults");
        Expect.That(new DepthState(false, CompareFunction.Always) != depth && depth == new DepthState(true, CompareFunction.Less), "depth equality");

        var raster = RasterState.defaultValue;
        Expect.That(raster.cullingMode == CullMode.Back && raster.offsetUnits == 0 && raster.offsetFactor == 0f && raster.depthClip, "raster defaults");
        var biased = new RasterState(CullMode.Front, 3, 1.5f, false);
        Expect.That(biased.cullingMode == CullMode.Front && biased.offsetUnits == 3 && biased.offsetFactor == 1.5f && !biased.depthClip, "raster arguments");
        Expect.That(biased != raster && raster == new RasterState(CullMode.Back, 0, 0f, true), "raster equality");
    }

    private static void Stencil()
    {
        var stencil = StencilState.defaultValue;
        Expect.That(!stencil.enabled && stencil.readMask == 255 && stencil.writeMask == 255, "stencil defaults: off, masks 255");
        Expect.That(stencil.compareFunctionFront == CompareFunction.Always && stencil.compareFunctionBack == CompareFunction.Always, "compare Always");
        Expect.That(stencil.passOperationFront == StencilOp.Keep && stencil.zFailOperationBack == StencilOp.Keep, "operations Keep");
        var set = new StencilState(true, 1, 2, CompareFunction.Equal, StencilOp.Replace, StencilOp.Zero, StencilOp.Invert);
        Expect.That(set.enabled && set.readMask == 1 && set.writeMask == 2, "enabled and masks");
        Expect.That(set.compareFunctionFront == CompareFunction.Equal && set.compareFunctionBack == CompareFunction.Equal, "compare function on both faces");
        Expect.That(set.passOperationFront == StencilOp.Replace && set.passOperationBack == StencilOp.Replace, "pass operation on both faces");
        Expect.That(set.failOperationFront == StencilOp.Zero && set.failOperationBack == StencilOp.Zero, "fail operation on both faces");
        Expect.That(set.zFailOperationFront == StencilOp.Invert && set.zFailOperationBack == StencilOp.Invert, "z-fail operation on both faces");
        set.SetCompareFunction(CompareFunction.Less);
        set.SetPassOperation(StencilOp.Keep);
        set.SetFailOperation(StencilOp.Invert);
        set.SetZFailOperation(StencilOp.Zero);
        Expect.That(set.compareFunctionFront == CompareFunction.Less && set.compareFunctionBack == CompareFunction.Less, "SetCompareFunction sets both faces");
        Expect.That(set.passOperationFront == StencilOp.Keep && set.passOperationBack == StencilOp.Keep, "SetPassOperation sets both faces");
        Expect.That(set.failOperationFront == StencilOp.Invert && set.failOperationBack == StencilOp.Invert, "SetFailOperation sets both faces");
        Expect.That(set.zFailOperationFront == StencilOp.Zero && set.zFailOperationBack == StencilOp.Zero, "SetZFailOperation sets both faces");
        var other = stencil;
        other.passOperationBack = StencilOp.Zero;
        Expect.That(other != stencil && stencil == StencilState.defaultValue, "stencil equality sees one face");
    }

    private static void RenderStateBlocks()
    {
        var block = new RenderStateBlock(RenderStateMask.Depth | RenderStateMask.Stencil);
        Expect.Equal(RenderStateMask.Depth | RenderStateMask.Stencil, block.mask, "mask");
        Expect.Equal(0, block.stencilReference, "stencil reference starts at 0");
        Expect.That(block.depthState == DepthState.defaultValue && block.stencilState == StencilState.defaultValue
            && block.rasterState == RasterState.defaultValue && block.blendState == BlendState.defaultValue, "states start at their defaults");
        var edited = block;
        edited.stencilReference = 7;
        Expect.That(edited != block && block == new RenderStateBlock(RenderStateMask.Depth | RenderStateMask.Stencil), "block equality sees the reference");
        edited = block;
        edited.depthState = new DepthState(false, CompareFunction.Always);
        Expect.That(edited != block, "block equality sees the depth state");
        Expect.That(new RenderStateBlock(RenderStateMask.Nothing) != block, "block equality sees the mask");
    }

    private static void Drawing()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.transform.position = new Vector3(1, 2, 3);
        var sorting = new SortingSettings(camera);
        Expect.Equal(new Vector3(1, 2, 3), sorting.cameraPosition, "camera position is copied");
        Expect.Equal(camera.worldToCameraMatrix, sorting.worldToCameraMatrix, "view matrix is copied");
        Expect.Equal(SortingCriteria.None, sorting.criteria, "no criteria");
        Expect.Equal(DistanceMetric.Perspective, sorting.distanceMetric, "perspective metric");
        Expect.Throws<ArgumentNullException>(() => new SortingSettings(null!), "a sorting setting needs a camera");
        var moved = sorting;
        moved.criteria = SortingCriteria.CommonOpaque;
        Expect.That(moved != sorting && sorting == new SortingSettings(camera), "sorting equality sees the criteria");

        var tag = new ShaderTagId("UniversalForward");
        var drawing = new DrawingSettings(tag, moved);
        Expect.Equal(tag, drawing.GetShaderPassName(0), "slot 0 holds the pass name");
        Expect.Equal(ShaderTagId.none, drawing.GetShaderPassName(1), "other slots hold no name");
        Expect.Equal(SortingCriteria.CommonOpaque, drawing.sortingSettings.criteria, "sorting settings are kept");
        Expect.Equal(-1, drawing.mainLightIndex, "main light index starts at -1");
        Expect.That(drawing.enableInstancing && !drawing.enableDynamicBatching, "instancing on, dynamic batching off");
        Expect.Equal(PerObjectData.None, drawing.perObjectData, "no per-object data");
        Expect.That(drawing.overrideMaterial == null && drawing.overrideShader == null && drawing.fallbackMaterial == null, "no overrides");
        var copy = drawing;
        copy.SetShaderPassName(15, new ShaderTagId("Last"));
        Expect.Equal(new ShaderTagId("Last"), copy.GetShaderPassName(15), "the last slot is writable");
        Expect.Equal(ShaderTagId.none, drawing.GetShaderPassName(15), "a copy has its own pass names");
        Expect.Throws<ArgumentOutOfRangeException>(() => drawing.GetShaderPassName(16), "slot 16");
        Expect.Throws<ArgumentOutOfRangeException>(() => copy.SetShaderPassName(-1, tag), "slot -1");
        drawing.perObjectData = PerObjectData.Lightmaps;
        drawing.overrideMaterialPassIndex = 2;
        drawing.mainLightIndex = 3;
        Expect.That(drawing.perObjectData == PerObjectData.Lightmaps && drawing.overrideMaterialPassIndex == 2 && drawing.mainLightIndex == 3, "settings are writable");
    }
}
