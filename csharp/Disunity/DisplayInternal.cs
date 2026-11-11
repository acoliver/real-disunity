namespace UnityEngineInternal;

/// <summary>The display queries the UI package's raycaster makes. The engine declares the type public so its module facade
/// can forward it.</summary>
public static class DisplayInternal
{
    /// <summary>Whether <paramref name="displayIndex"/> names a display other than the main one, index 0. The engine has one
    /// display (<see cref="UnityEngine.Display.displays"/>), so no index is a secondary display's (the engine's choice).</summary>
    public static bool IsASecondaryDisplayIndex(int displayIndex) =>
        displayIndex > 0 && displayIndex < UnityEngine.Display.displays.Length;
}
