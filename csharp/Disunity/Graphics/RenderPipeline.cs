namespace UnityEngine.Rendering;

/// <summary>A render pipeline asset. Only what <see cref="GraphicsSettings"/> needs exists so far; the asset
/// members a pipeline overrides come with the slice that loads the player's pipeline.</summary>
public abstract class RenderPipelineAsset : ScriptableObject
{
}

/// <summary>The settings asset of one render pipeline type.</summary>
public abstract class RenderPipelineGlobalSettings : ScriptableObject
{
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
