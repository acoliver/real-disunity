using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>A pipeline asset derives from the generic RenderPipelineAsset of its pipeline, as a render pipeline package's does.</summary>
internal static class RenderPipelineAssetTests
{
    private sealed class Pipeline : RenderPipeline { }

    private sealed class Asset : RenderPipelineAsset<Pipeline> { }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var asset = ScriptableObject.CreateInstance<Asset>();
            DisunityHost.SetRenderPipeline(asset, null);
            Expect.That(GraphicsSettings.currentRenderPipeline is RenderPipelineAsset, "the generic asset is a RenderPipelineAsset");
            Expect.That(GraphicsSettings.currentRenderPipeline is RenderPipelineAsset<Pipeline>, "and the asset of its pipeline");
            Expect.That(GraphicsSettings.isScriptableRenderPipelineEnabled, "a pipeline is enabled");
            Expect.Equal(typeof(Asset), GraphicsSettings.currentRenderPipelineAssetType, "the asset type");
            Expect.Equal(typeof(RenderPipelineAsset<>), typeof(Asset).BaseType!.GetGenericTypeDefinition(), "the base is the generic definition");
        }
        finally
        {
            DisunityHost.SetRenderPipeline(null, null);
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: a render pipeline asset derives from RenderPipelineAsset<TRenderPipeline>");
    }
}
