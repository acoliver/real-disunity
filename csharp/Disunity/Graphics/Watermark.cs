namespace UnityEngine.Rendering;

/// <summary>Whether a render pipeline draws the engine's watermark over the frame. The engine has no watermark, so it is never
/// visible. The engine declares the type public so its module facade can forward it.</summary>
public static class Watermark
{
    public static bool IsVisible() => false;
}
