namespace UnityEngine;

/// <summary>
/// The mouse cursor's state. It resets when a world loads: visible, not locked, default cursor. The
/// engine draws no cursor of its own; a presentation host reads the state through
/// <c>DisunityHost.CursorVisible</c> and <c>CursorLockState</c> and applies it.
/// </summary>
public static class Cursor
{
    private static readonly object Gate = new();
    private static bool _visible = true;
    private static CursorLockMode _lockState = CursorLockMode.None;
    private static Vector2 _hotspot;
    private static CursorMode _mode = CursorMode.Auto;
    private static Texture2D? _texture;

    public static bool visible
    {
        get { lock (Gate) return _visible; }
        set { lock (Gate) _visible = value; }
    }

    public static CursorLockMode lockState
    {
        get { lock (Gate) return _lockState; }
        set
        {
            if (!Enum.IsDefined(value)) throw new ArgumentOutOfRangeException(nameof(value), value, "unknown cursor lock mode");
            lock (Gate) _lockState = value;
        }
    }

    /// <summary>Records the texture, hotspot and mode, and a null texture restores the default cursor. The
    /// engine draws no cursor image; a presentation host reads <c>DisunityHost.CursorTexture</c> and applies it.</summary>
    public static void SetCursor(Texture2D texture, Vector2 hotspot, CursorMode cursorMode)
    {
        if (!Enum.IsDefined(cursorMode)) throw new ArgumentOutOfRangeException(nameof(cursorMode), cursorMode, "unknown cursor mode");
        lock (Gate) (_hotspot, _mode, _texture) = (hotspot, cursorMode, texture ? texture : null);
    }

    internal static Texture2D? Texture
    {
        get { lock (Gate) return _texture is { } value && value ? value : null; }
    }

    internal static Vector2 Hotspot
    {
        get { lock (Gate) return _hotspot; }
    }

    internal static CursorMode Mode
    {
        get { lock (Gate) return _mode; }
    }

    internal static void Reset()
    {
        lock (Gate) (_visible, _lockState, _hotspot, _mode, _texture) = (true, CursorLockMode.None, default, CursorMode.Auto, null);
    }
}
