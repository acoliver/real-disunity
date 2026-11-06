using UnityEngine;
using UnityEngine.Rendering;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>The asset the live pipeline was created for, or whose creation failed, so a failed asset is not
    /// retried every step. Null while no pipeline is wanted.</summary>
    private static RenderPipelineAsset? _pipelineAsset;

    /// <summary>
    /// Follows <see cref="GraphicsSettings.currentRenderPipeline"/> once per frame step, the engine's choice. A change
    /// of the active asset disposes the old pipeline in that step and the new one is created in the next; with an
    /// active asset and no pipeline for it, the asset's <c>CreatePipeline</c> runs. A creation that throws is reported
    /// once as a script exception and the asset is not tried again until it has stopped being active.
    /// </summary>
    private static void UpdateRenderPipeline()
    {
        var active = GraphicsSettings.currentRenderPipeline;
        if (_pipelineAsset is not null)
        {
            if (!ReferenceEquals(active, _pipelineAsset)) DisposeRenderPipeline();
        }
        else if (active is not null) CreateRenderPipeline(active);
    }

    private static void CreateRenderPipeline(RenderPipelineAsset asset)
    {
        _pipelineAsset = asset;
        _renderFailures = new RenderFailures();
        try { RenderPipelineManager.Activate(asset.InvokeCreatePipeline()); }
        catch (Exception error) when (IsGameException(error)) { ReportScriptException(error, asset); }
    }

    /// <summary>Disposes the live pipeline, if any, and forgets the asset it came from. World unload and an asset
    /// change both end here.</summary>
    private static void DisposeRenderPipeline()
    {
        _pipelineAsset = null;
        _renderFailures = new RenderFailures();
        var pipeline = RenderPipelineManager.Deactivate();
        if (pipeline is null) return;
        try
        {
            pipeline.Dispose();
            RenderPipelineManager.RaiseDisposed();
        }
        catch (Exception error) when (IsGameException(error)) { ReportScriptException(error, null); }
    }
}
