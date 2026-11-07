using UnityEngine.Experimental.Rendering;

namespace UnityEngine;

/// <summary>
/// The HDR output state of a display. The engine has one display with no HDR output: <see cref="available"/> and
/// <see cref="active"/> are false, <see cref="RequestHDRModeChange"/> changes nothing (the engine's choice) and the
/// luminance range is unreported (0). <see cref="automaticHDRTonemapping"/> and <see cref="paperWhiteNits"/> are stored.
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

    /// <summary>Accepted and ignored: the display cannot output HDR.</summary>
    public void RequestHDRModeChange(bool enabled) { }
}
