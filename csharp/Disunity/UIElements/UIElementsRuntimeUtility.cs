namespace UnityEngine.UIElements;

/// <summary>
/// The player's registry of UI Toolkit panels. A world has no UI Toolkit panels, so the registry is
/// always empty and an event system drives uGUI only.
/// </summary>
public static class UIElementsRuntimeUtility
{
    /// <summary>The player panels in display order: none.</summary>
    public static List<BaseRuntimePanel> GetSortedPlayerPanels() => new();
}
