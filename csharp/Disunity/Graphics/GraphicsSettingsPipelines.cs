namespace UnityEngine.Rendering;

/// <summary>
/// Two registries. The global settings assets are kept per render pipeline type, as a host or a player's
/// loading registers them. The graphics settings groups (<see cref="IRenderPipelineGraphicsSettings"/>) are
/// added by the loader from the player's global settings asset. With nothing registered, TryGet returns false
/// and GetRenderPipelineSettings returns null, so scripts that test for a group before using it keep working;
/// Unity's page does not say whether the unregistered case throws, and a null return is the safe reading.
/// </summary>
public sealed partial class GraphicsSettings
{
    private static readonly Dictionary<Type, RenderPipelineGlobalSettings> GlobalSettings = new();
    private static readonly List<IRenderPipelineGraphicsSettings> SettingsGroups = [];

    internal static void AddGraphicsSettings(IRenderPipelineGraphicsSettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        lock (Gate)
        {
            SettingsGroups.RemoveAll(existing => existing.GetType() == settings.GetType());
            SettingsGroups.Add(settings);
        }
    }

    public static T? GetRenderPipelineSettings<T>() where T : class, IRenderPipelineGraphicsSettings =>
        TryGetRenderPipelineSettings<T>(out var settings) ? settings : null;

    public static bool TryGetRenderPipelineSettings<T>(out T? settings) where T : class, IRenderPipelineGraphicsSettings
    {
        lock (Gate) settings = SettingsGroups.OfType<T>().FirstOrDefault();
        return settings is not null;
    }

    public static void RegisterRenderPipelineSettings(Type renderPipelineType, RenderPipelineGlobalSettings settings)
    {
        ArgumentNullException.ThrowIfNull(renderPipelineType);
        ArgumentNullException.ThrowIfNull(settings);
        lock (Gate) GlobalSettings[renderPipelineType] = settings;
    }

    public static void RegisterRenderPipelineSettings<TRenderPipeline, TGlobalSettings>(TGlobalSettings settings)
        where TRenderPipeline : RenderPipeline
        where TGlobalSettings : RenderPipelineGlobalSettings =>
        RegisterRenderPipelineSettings(typeof(TRenderPipeline), settings);

    public static void UnregisterRenderPipelineSettings(Type renderPipelineType)
    {
        ArgumentNullException.ThrowIfNull(renderPipelineType);
        lock (Gate) GlobalSettings.Remove(renderPipelineType);
    }

    public static void UnregisterRenderPipelineSettings<TRenderPipeline>() where TRenderPipeline : RenderPipeline =>
        UnregisterRenderPipelineSettings(typeof(TRenderPipeline));

    public static RenderPipelineGlobalSettings? GetSettingsForRenderPipeline(Type renderPipelineType)
    {
        ArgumentNullException.ThrowIfNull(renderPipelineType);
        lock (Gate) return GlobalSettings.TryGetValue(renderPipelineType, out var settings) ? settings : null;
    }

    public static RenderPipelineGlobalSettings? GetSettingsForRenderPipeline<TRenderPipeline>() where TRenderPipeline : RenderPipeline =>
        GetSettingsForRenderPipeline(typeof(TRenderPipeline));

    /// <summary>Registers the asset for the pipeline type, replacing the earlier one; the engine does not write assets to disk.</summary>
    public static void UpdateGraphicsSettings(RenderPipelineGlobalSettings newSettings, Type renderPipelineType)
    {
        if (newSettings is null) UnregisterRenderPipelineSettings(renderPipelineType);
        else RegisterRenderPipelineSettings(renderPipelineType, newSettings);
    }

    public static bool TrySetCurrentRenderPipelineGlobalSettings(RenderPipelineGlobalSettings asset) => throw CurrentPipelineType(nameof(TrySetCurrentRenderPipelineGlobalSettings));

    public static bool TryGetCurrentRenderPipelineGlobalSettings(out RenderPipelineGlobalSettings? asset) => throw CurrentPipelineType(nameof(TryGetCurrentRenderPipelineGlobalSettings));

    private static NotSupportedException CurrentPipelineType(string member) =>
        new($"GraphicsSettings.{member} is not supported yet: it needs the current asset's pipeline type, which comes with the slice that loads the player's render pipeline. Use the overloads that take the pipeline type.");
}
