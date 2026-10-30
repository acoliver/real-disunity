using UnityEngine.Rendering;

namespace UnityEngine.XR;

/// <summary>
/// The display of an XR device. The engine has no XR device and <see cref="SubsystemManager"/> never returns
/// one, so a game sees "no XR display". An instance can still be constructed; every member that would talk
/// to a device throws <see cref="NotSupportedException"/> naming the member.
/// </summary>
public partial class XRDisplaySubsystem : IntegratedSubsystem<XRDisplaySubsystemDescriptor>
{
    public float zNear
    {
        get => throw Unsupported(nameof(zNear));
        set => throw Unsupported(nameof(zNear));
    }

    public float zFar
    {
        get => throw Unsupported(nameof(zFar));
        set => throw Unsupported(nameof(zFar));
    }

    public bool sRGB
    {
        get => throw Unsupported(nameof(sRGB));
        set => throw Unsupported(nameof(sRGB));
    }

    public float occlusionMaskScale
    {
        get => throw Unsupported(nameof(occlusionMaskScale));
        set => throw Unsupported(nameof(occlusionMaskScale));
    }

    public bool singlePassRenderingDisabled
    {
        get => throw Unsupported(nameof(singlePassRenderingDisabled));
        set => throw Unsupported(nameof(singlePassRenderingDisabled));
    }

    public bool disableLegacyRenderer
    {
        get => throw Unsupported(nameof(disableLegacyRenderer));
        set => throw Unsupported(nameof(disableLegacyRenderer));
    }

    public bool contentProtectionEnabled
    {
        get => throw Unsupported(nameof(contentProtectionEnabled));
        set => throw Unsupported(nameof(contentProtectionEnabled));
    }

    public float foveatedRenderingLevel
    {
        get => throw Unsupported(nameof(foveatedRenderingLevel));
        set => throw Unsupported(nameof(foveatedRenderingLevel));
    }

    public TextureLayout textureLayout
    {
        get => throw Unsupported(nameof(textureLayout));
        set => throw Unsupported(nameof(textureLayout));
    }

    public ReprojectionMode reprojectionMode
    {
        get => throw Unsupported(nameof(reprojectionMode));
        set => throw Unsupported(nameof(reprojectionMode));
    }

    public TextureLayout supportedTextureLayouts => throw Unsupported(nameof(supportedTextureLayouts));

    public bool displayOpaque => throw Unsupported(nameof(displayOpaque));

    public float scaleOfAllViewports
    {
        set => throw Unsupported(nameof(scaleOfAllViewports));
    }

    public float scaleOfAllRenderTargets
    {
        set => throw Unsupported(nameof(scaleOfAllRenderTargets));
    }

    public int GetRenderPassCount() => throw Unsupported(nameof(GetRenderPassCount));

    public void GetRenderPass(int renderPassIndex, out XRRenderPass renderPass) => throw Unsupported(nameof(GetRenderPass));

    public bool GetCullingParameters(Camera camera, int cullingPassIndex, out ScriptableCullingParameters scriptableCullingParameters) =>
        throw Unsupported(nameof(GetCullingParameters));

    public RenderTexture GetRenderTextureForRenderPass(int renderPass) => throw Unsupported(nameof(GetRenderTextureForRenderPass));

    public bool GetMirrorViewBlitDesc(RenderTexture mirrorRt, out XRMirrorViewBlitDesc outDesc) =>
        throw Unsupported(nameof(GetMirrorViewBlitDesc));

    public bool GetMirrorViewBlitDesc(RenderTexture mirrorRt, out XRMirrorViewBlitDesc outDesc, int mode) =>
        throw Unsupported(nameof(GetMirrorViewBlitDesc));

    public bool AddGraphicsThreadMirrorViewBlit(CommandBuffer cmd, bool allowGraphicsStateInvalidate) =>
        throw Unsupported(nameof(AddGraphicsThreadMirrorViewBlit));

    public bool AddGraphicsThreadMirrorViewBlit(CommandBuffer cmd, bool allowGraphicsStateInvalidate, int mode) =>
        throw Unsupported(nameof(AddGraphicsThreadMirrorViewBlit));

    public int GetPreferredMirrorBlitMode() => throw Unsupported(nameof(GetPreferredMirrorBlitMode));

    public void SetPreferredMirrorBlitMode(int blitMode) => throw Unsupported(nameof(SetPreferredMirrorBlitMode));

    public void GetFocusPlane(out Vector3 point, out Vector3 normal, out Vector3 velocity) => throw Unsupported(nameof(GetFocusPlane));

    public void SetFocusPlane(Vector3 point, Vector3 normal, Vector3 velocity) => throw Unsupported(nameof(SetFocusPlane));

    public void SetMSAALevel(int level) => throw Unsupported(nameof(SetMSAALevel));

    public int GetMSAALevel() => throw Unsupported(nameof(GetMSAALevel));

    public void MarkTransformLateLatched(Transform transform, LateLatchNode nodeType) =>
        throw Unsupported(nameof(MarkTransformLateLatched));

    private static NotSupportedException Unsupported(string member) => NoXrDevice.Member($"{nameof(XRDisplaySubsystem)}.{member}");
}

/// <summary>Describes the XR display subsystem. The engine registers none, and <c>Create</c> throws.</summary>
public class XRDisplaySubsystemDescriptor : IntegratedSubsystemDescriptor<XRDisplaySubsystem>
{
    public bool disablesLegacyVr => false;

    public bool enableBackBufferMSAA => false;
}
