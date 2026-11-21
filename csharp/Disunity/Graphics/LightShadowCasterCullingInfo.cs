namespace UnityEngine.Rendering;

/// <summary>What shadow caster culling needs to know about one visible light: <see cref="splitRange"/> is the run of the shadow
/// splits that belong to it and <see cref="projectionType"/> the projection of the light's shadow maps. The engine culls a light's casters from the split data of <see cref="ShadowDrawingSettings"/>, so it
/// reads neither.</summary>
public struct LightShadowCasterCullingInfo
{
    public RangeInt splitRange;
    public BatchCullingProjectionType projectionType;
}
