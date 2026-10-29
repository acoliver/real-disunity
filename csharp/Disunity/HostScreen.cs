using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    internal const float DefaultDpi = 96f;
    internal static float ScreenDpi { get; private set; } = DefaultDpi;
    internal static RefreshRate ScreenRefreshRate { get; private set; } = new() { numerator = 60, denominator = 1 };

    /// <summary>
    /// Tells <see cref="Screen"/> and <see cref="Display"/> the size of the presentation surface,
    /// its pixel density and its refresh rate. This is the viewport UI layout uses, so the two never differ.
    /// A host that has no window runs at <see cref="SetViewport"/>'s default of 800 by 600; a headless
    /// player run calls this with 1920 by 1080 at 96 dpi and 60 Hz, Unity's default standalone window.
    /// </summary>
    public static void SetScreen(int width, int height, float dpi, float refreshRate)
    {
        if (width <= 0 || height <= 0) throw new ArgumentOutOfRangeException(nameof(width), "a screen has pixels");
        if (!float.IsFinite(dpi) || dpi <= 0) throw new ArgumentOutOfRangeException(nameof(dpi));
        if (!float.IsFinite(refreshRate) || refreshRate <= 0) throw new ArgumentOutOfRangeException(nameof(refreshRate));
        SetViewport((uint)width, (uint)height);
        ScreenDpi = dpi;
        ScreenRefreshRate = new RefreshRate { numerator = (uint)Math.Round(refreshRate * 1000), denominator = 1000 };
    }
}
