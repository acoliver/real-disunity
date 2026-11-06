namespace UnityEngine.VFX;

/// <summary>How a camera's views split between eyes for visual effects: the engine has no stereo, so a camera is one view
/// (<c>viewTotal</c> 1, <c>viewCount</c> 1, <c>viewOffset</c> 0) when a pipeline fills it in. Plain data.</summary>
public struct VFXCameraXRSettings
{
    public uint viewTotal;
    public uint viewCount;
    public uint viewOffset;
}
