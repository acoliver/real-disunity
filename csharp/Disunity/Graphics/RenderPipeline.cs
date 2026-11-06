using Disunity.Hosting;

namespace UnityEngine.Rendering;

/// <summary>A render pipeline asset. A pipeline package's asset overrides <see cref="CreatePipeline"/>, which the host
/// calls once for the active asset to build the live pipeline (see <see cref="RenderPipelineManager.currentPipeline"/>).</summary>
public abstract class RenderPipelineAsset : ScriptableObject
{
    /// <summary>Builds the render pipeline this asset describes. The host calls it on the first frame step where the
    /// asset is the active one, and again only after the asset has stopped being active and become active anew.</summary>
    protected abstract RenderPipeline CreatePipeline();

    /// <summary>Lets the host call <see cref="CreatePipeline"/>.</summary>
    internal RenderPipeline InvokeCreatePipeline() => CreatePipeline();

    /// <summary>Called when the asset is disabled; a pipeline asset overrides it and calls this base.</summary>
    protected virtual void OnDisable() { }

    /// <summary>The pipeline type argument of the asset's <see cref="RenderPipelineAsset{TRenderPipeline}"/> base,
    /// which names the pipeline the asset creates and the global settings it uses; null for an asset without one.</summary>
    internal Type? PipelineType
    {
        get
        {
            for (var type = GetType(); type is not null; type = type.BaseType)
                if (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(RenderPipelineAsset<>)) return type.GenericTypeArguments[0];
            return null;
        }
    }
}

/// <summary>The asset of a render pipeline type; a render pipeline package derives its asset from this. It adds
/// no members and no constraint on the pipeline type: the pipeline an asset creates comes with the slice that renders through a scriptable pipeline.</summary>
public abstract class RenderPipelineAsset<TRenderPipeline> : RenderPipelineAsset
{
}

/// <summary>The settings asset of one render pipeline type.</summary>
public abstract class RenderPipelineGlobalSettings : ScriptableObject
{
    /// <summary>How many container objects deep a <see cref="RenderPipelineGraphicsSettingsCollection"/> is looked for.</summary>
    private const int MaxContainerDepth = 3;

    /// <summary>The settings groups the asset holds: the entries of every <see cref="RenderPipelineGraphicsSettingsCollection"/>
    /// in its serialized fields, or in the serialized fields of a settings container object those fields hold (a pipeline
    /// package keeps its collections in a container), in field order.</summary>
    internal IEnumerable<IRenderPipelineGraphicsSettings> GraphicsSettingsGroups() =>
        CollectionsIn(this, 0)
            .SelectMany(collection => collection.settingsList)
            .OfType<IRenderPipelineGraphicsSettings>();

    private static IEnumerable<RenderPipelineGraphicsSettingsCollection> CollectionsIn(object owner, int depth)
    {
        foreach (var field in DisunityHost.SerializedFields(owner.GetType()))
        {
            var value = field.GetValue(owner);
            if (value is RenderPipelineGraphicsSettingsCollection collection) yield return collection;
            else if (depth < MaxContainerDepth && IsContainer(value))
                foreach (var inner in CollectionsIn(value, depth + 1)) yield return inner;
        }
    }

    private static bool IsContainer([System.Diagnostics.CodeAnalysis.NotNullWhen(true)] object? value) =>
        value is not null and not UnityEngine.Object and not string and not System.Collections.IEnumerable
        && !value.GetType().IsPrimitive && !value.GetType().IsEnum;
}

/// <summary>A render pipeline. The host creates it from the active <see cref="RenderPipelineAsset"/> and disposes it when
/// the asset stops being active or the world unloads. It calls <see cref="Render(ScriptableRenderContext, List{Camera})"/>
/// once per frame step, after the script phases, with the enabled cameras by depth (see DisunityHost.RenderFrame).</summary>
public abstract class RenderPipeline
{
    public bool disposed { get; private set; }

    /// <summary>Renders the cameras into the context; a pipeline implements it. The render pipeline package the
    /// player bundles implements this form, so it is the abstract one.</summary>
    protected abstract void Render(ScriptableRenderContext context, List<Camera> cameras);

    /// <summary>The array form of <see cref="Render(ScriptableRenderContext, List{Camera})"/>; by default it calls the list form.</summary>
    protected virtual void Render(ScriptableRenderContext context, Camera[] cameras) => Render(context, new List<Camera>(cameras));

    /// <summary>Lets the host call the list form.</summary>
    internal void InvokeRender(ScriptableRenderContext context, List<Camera> cameras) => Render(context, cameras);

    /// <summary>Does nothing: the host raises <see cref="RenderPipelineManager.beginContextRendering"/> before it calls
    /// Render, so a pipeline that calls this does not raise the event a second time. The engine's choice.</summary>
    protected static void BeginContextRendering(ScriptableRenderContext context, List<Camera> cameras) { }

    /// <summary>Does nothing; see <see cref="BeginContextRendering"/>. The host raises the end event after Render returns.</summary>
    protected static void EndContextRendering(ScriptableRenderContext context, List<Camera> cameras) { }

    /// <summary>Raises <see cref="RenderPipelineManager.beginCameraRendering"/>.</summary>
    protected static void BeginCameraRendering(ScriptableRenderContext context, Camera camera) =>
        RenderPipelineManager.RaiseBeginCamera(context, camera);

    /// <summary>Raises <see cref="RenderPipelineManager.endCameraRendering"/>.</summary>
    protected static void EndCameraRendering(ScriptableRenderContext context, Camera camera) =>
        RenderPipelineManager.RaiseEndCamera(context, camera);

    /// <summary>Raises <see cref="RenderPipelineManager.beginFrameRendering"/>.</summary>
    protected static void BeginFrameRendering(ScriptableRenderContext context, Camera[] cameras) =>
        RenderPipelineManager.RaiseBeginFrame(context, cameras);

    /// <summary>Raises <see cref="RenderPipelineManager.endFrameRendering"/>.</summary>
    protected static void EndFrameRendering(ScriptableRenderContext context, Camera[] cameras) =>
        RenderPipelineManager.RaiseEndFrame(context, cameras);

    /// <summary>Releases the pipeline's resources. <see cref="Dispose()"/> calls it with true, once.</summary>
    protected virtual void Dispose(bool disposing) { }

    /// <summary>Disposes the pipeline; a second call does nothing.</summary>
    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        Dispose(true);
        GC.SuppressFinalize(this);
    }
}

/// <summary>A group of settings stored in a pipeline's global settings. Unity's interface requires all three
/// members; the last two have defaults here so a settings class need not repeat them.</summary>
public interface IRenderPipelineGraphicsSettings
{
    int version { get; }

    bool isAvailableInPlayerBuild => true;

    void Reset() { }
}

/// <summary>A group of settings that holds the resources a render pipeline loads, such as shaders and
/// materials. The version and the player-build flag are those of <see cref="IRenderPipelineGraphicsSettings"/>.</summary>
public interface IRenderPipelineResources : IRenderPipelineGraphicsSettings
{
}

/// <summary>The settings groups a global settings asset holds. The list holds each group by
/// <c>[SerializeReference]</c>, so a group's class is whatever the data names.</summary>
[Serializable]
public class RenderPipelineGraphicsSettingsCollection
{
    [SerializeReference] private List<IRenderPipelineGraphicsSettings> m_List = new();

    public List<IRenderPipelineGraphicsSettings> settingsList => m_List;
}
