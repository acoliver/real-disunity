using UnityEngine.Rendering;

namespace UnityEngine.XR;

public partial class XRDisplaySubsystem
{
    [Flags]
    public enum TextureLayout
    {
        Texture2DArray = 1,
        SeparateTexture2Ds = 2,
        SingleTexture2D = 4,
    }

    public enum ReprojectionMode
    {
        Unspecified = 0,
        None = 1,
        PositionAndOrientation = 2,
        OrientationOnly = 3,
    }

    public enum LateLatchNode
    {
        Head = 0,
        LeftHand = 1,
        RightHand = 2,
    }

    /// <summary>The mirror view modes, as the integer codes the blit calls take.</summary>
    public static class XRMirrorViewBlitMode
    {
        public const int None = 0;
        public const int Left = -1;
        public const int Right = -2;
        public const int SideBySide = -3;
    }

    /// <summary>One eye or view of a render pass. A plain data value.</summary>
    public struct XRRenderParameter
    {
        public Matrix4x4 view;
        public Matrix4x4 projection;
        public Rect viewport;
        public Mesh occlusionMesh;
        public int textureArraySlice;
        public Matrix4x4 previousView;
        public bool isPreviousViewValid;
    }

    /// <summary>
    /// A render pass of the display. The engine has no XR device, so a pass holds no render parameters:
    /// <see cref="GetRenderParameterCount"/> is 0 and every index is out of range.
    /// </summary>
    public struct XRRenderPass
    {
        public int renderPassIndex;
        public RenderTargetIdentifier renderTarget;
        public RenderTextureDescriptor renderTargetDesc;
        public bool hasMotionVectorPass;
        public RenderTargetIdentifier motionVectorRenderTarget;
        public RenderTextureDescriptor motionVectorRenderTargetDesc;
        public bool shouldFillOutDepth;
        public int cullingPassIndex;

        public readonly int GetRenderParameterCount() => 0;

        public readonly void GetRenderParameter(Camera camera, int renderParameterIndex, out XRRenderParameter renderParameter) =>
            throw new ArgumentOutOfRangeException(nameof(renderParameterIndex), renderParameterIndex,
                "XRRenderPass.GetRenderParameter: the engine has no XR device, so a render pass has no render parameters");
    }

    /// <summary>One copy of a mirror view blit. A plain data value.</summary>
    public struct XRBlitParams
    {
        public Texture srcTex;
        public int srcTexArraySlice;
        public Rect srcRect;
        public Rect destRect;
        public IntPtr foveatedRenderingInfo;
    }

    /// <summary>
    /// How a mirror view is blitted. The engine has no XR device, so the description holds no blit
    /// parameters: every index of <see cref="GetBlitParameter"/> is out of range.
    /// </summary>
    public struct XRMirrorViewBlitDesc
    {
        public bool nativeBlitAvailable;
        public bool nativeBlitInvalidStates;
        public int blitParamsCount;

        public readonly void GetBlitParameter(int blitParameterIndex, out XRBlitParams blitParameter) =>
            throw new ArgumentOutOfRangeException(nameof(blitParameterIndex), blitParameterIndex,
                "XRMirrorViewBlitDesc.GetBlitParameter: the engine has no XR device, so a blit description has no blit parameters");
    }
}
