namespace UnityEngine.Rendering;

/// <summary>The rendering features the active render pipeline supports. A pipeline replaces <see cref="active"/> when it
/// is created and sets the flags it differs on; the engine only holds the values and reads none of them.</summary>
public class SupportedRenderingFeatures
{
    private static SupportedRenderingFeatures _active = new();

    /// <summary>The features of the active pipeline; a new set of defaults until a pipeline replaces it.</summary>
    public static SupportedRenderingFeatures active
    {
        get => Volatile.Read(ref _active);
        set => Volatile.Write(ref _active, value ?? throw new ArgumentNullException(nameof(value)));
    }

    public bool overridesEnvironmentLighting { get; set; }
    public bool overridesFog { get; set; }
    public bool overridesOtherLightingSettings { get; set; }
    public bool overridesLODBias { get; set; }
    public bool overridesMaximumLODLevel { get; set; }
    public bool overridesEnableLODCrossFade { get; set; }
    public bool overridesRealtimeReflectionProbes { get; set; }
    public bool overridesLightProbeSystem { get; set; }
    public bool overridesShadowmask { get; set; }
    public bool rendersUIOverlay { get; set; }
    public bool editableMaterialRenderQueue { get; set; } = true;
    public bool autoAmbientProbeBaking { get; set; } = true;
    public bool autoDefaultReflectionProbeBaking { get; set; } = true;
    public bool lightProbeProxyVolumes { get; set; } = true;
    public bool motionVectors { get; set; } = true;
    public bool receiveShadows { get; set; } = true;
    public bool reflectionProbes { get; set; } = true;
    public bool reflectionProbesBlendDistance { get; set; } = true;
    public bool particleSystemInstancing { get; set; } = true;
}
