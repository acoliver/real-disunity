using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Disunity.Hosting;

/// <summary>
/// The render pipeline assets a player starts with, from the <c>graphics</c> section of the resource plan:
/// <c>{"render_pipeline": asset or null, "global_settings": asset or null, "quality_pipelines": [asset or null per
/// quality level]}</c>. They are script assets like any other, built with their fields. An asset that cannot be
/// built is logged by name with its cause and left null, so the player starts without it.
/// </summary>
internal sealed partial class ResourceDatabase
{
    /// <summary>Builds the planned pipeline assets and gives them to <see cref="GraphicsSettings"/>,
    /// <see cref="QualitySettings"/> and the global settings registry. Building does not create a renderer.</summary>
    public void ApplyGraphics(Action<string> log)
    {
        RequireMainThread();
        if (_graphics is not { } graphics) return;
        var pipeline = BuildGraphicsAsset<RenderPipelineAsset>(Planned(graphics, "render_pipeline"), "render pipeline asset", log);
        var global = BuildGraphicsAsset<RenderPipelineGlobalSettings>(Planned(graphics, "global_settings"), "render pipeline global settings asset", log);
        var levels = graphics.GetProperty("quality_pipelines").EnumerateArray().Select(level => level.ValueKind == System.Text.Json.JsonValueKind.Null ? (int?)null : level.GetInt32()).ToArray();
        var quality = levels.Select(index => BuildGraphicsAsset<RenderPipelineAsset>(index, "quality level render pipeline asset", log)).ToArray();
        GraphicsSettings.SetPipelineAssets(pipeline, null);
        if (quality.Length > 0) QualitySettings.SetPipelines(quality);
        if (pipeline) log($"render pipeline asset: {pipeline!.name} ({pipeline.GetType().FullName})");
        RegisterGlobalSettings(pipeline, global, log);
    }

    private static int? Planned(System.Text.Json.JsonElement graphics, string name) =>
        graphics.GetProperty(name) is { ValueKind: not System.Text.Json.JsonValueKind.Null } index ? index.GetInt32() : null;

    private T? BuildGraphicsAsset<T>(int? index, string what, Action<string> log) where T : Object
    {
        if (index is not { } asset) return null;
        var name = _assets[asset].Str("name");
        try
        {
            var built = Build(new Entry("", "", asset), typeof(T));
            if (built is T typed) return typed;
            log($"the {what} {name} is a {built.GetType().FullName}, not a {typeof(T).Name}; the player starts without it");
        }
        catch (Exception error)
        {
            // Creating the instance wraps an exception of its constructor.
            while (error is System.Reflection.TargetInvocationException { InnerException: { } inner }) error = inner;
            log($"the {what} {name} could not be built: {error.GetType().Name}: {error.Message}; the player starts without it");
        }
        return null;
    }

    /// <summary>Registers the global settings under the pipeline type of the pipeline asset, which is the type argument
    /// of the asset's <see cref="RenderPipelineAsset{TRenderPipeline}"/> base.</summary>
    private static void RegisterGlobalSettings(RenderPipelineAsset? pipeline, RenderPipelineGlobalSettings? global, Action<string> log)
    {
        if (!global) return;
        var type = pipeline?.PipelineType;
        if (type is null)
        {
            log($"the global settings {global!.name} are not registered: the pipeline asset names no pipeline type");
            return;
        }
        GraphicsSettings.RegisterRenderPipelineSettings(type, global!);
        log($"render pipeline global settings: {global!.name} for {type.FullName}");
    }
}
