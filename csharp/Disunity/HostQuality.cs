using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>Names the quality levels <see cref="QualitySettings.names"/> reports and picks the current one.</summary>
    public static void SetQualityLevels(IReadOnlyList<string> names, int current) => QualitySettings.SetLevels(names, current);

    /// <summary>Sets the color space <see cref="QualitySettings.activeColorSpace"/> reports.</summary>
    public static void SetColorSpace(ColorSpace space) => QualitySettings.SetColorSpace(space);

    /// <summary>Whether the presentation shows the cursor.</summary>
    public static bool CursorVisible => Cursor.visible;

    /// <summary>Whether the presentation should lock or confine the cursor.</summary>
    public static CursorLockMode CursorLockState => Cursor.lockState;

    /// <summary>The hotspot a script gave <see cref="Cursor.SetCursor"/>.</summary>
    public static Vector2 CursorHotspot => Cursor.Hotspot;
    /// <summary>The texture a script gave <see cref="Cursor.SetCursor"/>, or null for the default cursor.</summary>
    public static Texture2D? CursorTexture => Cursor.Texture;

    /// <summary>The cursor mode a script gave <see cref="Cursor.SetCursor"/>.</summary>
    public static CursorMode CursorMode => Cursor.Mode;

    /// <summary>Settings a world owns return to their defaults when it loads.</summary>
    private static void ResetWorldSettings()
    {
        QualitySettings.Reset();
        Cursor.Reset();
    }
}
