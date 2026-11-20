using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>What a pipeline passes to <see cref="ScriptableRenderContext.CullShadowCasters"/> to describe the shadow caster culling
/// it wants: the shadow splits of all its lights in <see cref="splitBuffer"/> and, for each visible light, the run of them that is
/// its own in <see cref="perLightInfos"/>. The engine culls a light's casters from the split data of
/// <see cref="ShadowDrawingSettings"/>, so it does not read either.</summary>
public struct ShadowCastersCullingInfos
{
    public NativeArray<ShadowSplitData> splitBuffer;
    public NativeArray<LightShadowCasterCullingInfo> perLightInfos;
}
