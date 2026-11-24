using UnityEngine.Experimental.Rendering;

namespace UnityEngine;

/// <summary>
/// The HDR output state of a display. The engine has one display with no HDR output: <see cref="available"/> and
/// <see cref="active"/> are false, <see cref="RequestHDRModeChange"/> refuses a request to turn HDR on because the display cannot
/// enter an HDR mode, and the luminance range is 0 because the display reports none. <see cref="automaticHDRTonemapping"/> and <see cref="paperWhiteNits"/> are stored.
/// </summary>
public sealed class HDROutputSettings
{
    private static readonly HDROutputSettings Main = new();
    private static readonly HDROutputSettings[] Displays = [Main];

    public static HDROutputSettings main => Main;
    public static HDROutputSettings[] displays => Displays;

    public bool available => false;
    public bool active => false;
    public bool HDRModeChangeRequested => false;
    public bool automaticHDRTonemapping { get; set; } = true;
    public int paperWhiteNits { get; set; } = 160;
    public float minToneMapLuminance => 0f;
    public float maxToneMapLuminance => 0f;
    public float maxFullFrameToneMapLuminance => 0f;
    public ColorGamut displayColorGamut => ColorGamut.sRGB;
    public GraphicsFormat graphicsFormat => GraphicsFormat.R8G8B8A8_UNorm;
    public GraphicsFormat format => graphicsFormat;

    /// <summary>A request to turn HDR off finds it off already, so nothing changes. A request to turn it on throws
    /// <see cref="NotSupportedException"/> naming this member: the display cannot output HDR and <see cref="available"/> says so.</summary>
    public void RequestHDRModeChange(bool enabled)
    {
        if (enabled)
            throw new NotSupportedException("HDROutputSettings.RequestHDRModeChange is not supported yet for turning HDR on: the engine's display cannot output HDR (available is false)");
    }
}
