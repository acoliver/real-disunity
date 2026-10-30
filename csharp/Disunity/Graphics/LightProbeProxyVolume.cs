namespace UnityEngine;

/// <summary>
/// A volume that lights a large object with probes. Only the type exists, so renderers and draw calls can
/// refer to it; it cannot be added to a GameObject and its members are not provided.
/// </summary>
public sealed class LightProbeProxyVolume : Behaviour
{
    private protected override bool EnabledState { get; set; } = true;
}
