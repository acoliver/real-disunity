using Disunity.Hosting;

namespace UnityEngine.Rendering;

/// <summary>A render pipeline asset. Only what <see cref="GraphicsSettings"/> needs exists so far; the asset
/// members a pipeline overrides come with the slice that loads the player's pipeline.</summary>
public abstract class RenderPipelineAsset : ScriptableObject
{
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
    /// <summary>The settings groups the asset holds: the entries of every <see cref="RenderPipelineGraphicsSettingsCollection"/>
    /// in its serialized fields, in field order.</summary>
    internal IEnumerable<IRenderPipelineGraphicsSettings> GraphicsSettingsGroups() =>
        DisunityHost.SerializedFields(GetType())
            .Where(field => field.FieldType == typeof(RenderPipelineGraphicsSettingsCollection))
            .Select(field => (RenderPipelineGraphicsSettingsCollection?)field.GetValue(this))
            .OfType<RenderPipelineGraphicsSettingsCollection>()
            .SelectMany(collection => collection.settingsList)
            .OfType<IRenderPipelineGraphicsSettings>();
}

/// <summary>A render pipeline. Its rendering members come with the slice that loads the player's pipeline.</summary>
public abstract class RenderPipeline
{
    public bool disposed { get; private set; }

    public void Dispose() => disposed = true;
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
    [SerializeReference] private List<IRenderPipelineGraphicsSettings> m_SettingsList = new();

    public List<IRenderPipelineGraphicsSettings> settingsList => m_SettingsList;
}
