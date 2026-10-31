using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The settings for one <see cref="Graphics.RenderMesh"/>-style draw. The struct only stores them: the engine
/// draws nothing through those calls yet. The defaults are the engine's choices, except that a draw
/// casts and receives shadows and uses the camera's motion vectors.
/// </summary>
public struct RenderParams
{
    public int layer;
    public uint renderingLayerMask;
    public int rendererPriority;
    public Bounds worldBounds;
    public Camera? camera;
    public MotionVectorGenerationMode motionVectorMode;
    public ReflectionProbeUsage reflectionProbeUsage;
    public Material material;
    public MaterialPropertyBlock? matProps;
    public ShadowCastingMode shadowCastingMode;
    public bool receiveShadows;
    public LightProbeUsage lightProbeUsage;
    public LightProbeProxyVolume? lightProbeProxyVolume;

    public RenderParams(Material mat)
    {
        layer = 0;
        renderingLayerMask = 1;
        rendererPriority = 0;
        worldBounds = new Bounds(Vector3.zero, new Vector3(10000f, 10000f, 10000f));
        camera = null;
        motionVectorMode = MotionVectorGenerationMode.Camera;
        reflectionProbeUsage = ReflectionProbeUsage.Off;
        material = mat;
        matProps = null;
        shadowCastingMode = ShadowCastingMode.On;
        receiveShadows = true;
        lightProbeUsage = LightProbeUsage.Off;
        lightProbeProxyVolume = null;
    }
}
