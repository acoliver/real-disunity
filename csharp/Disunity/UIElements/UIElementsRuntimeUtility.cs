namespace UnityEngine.UIElements;

/// <summary>
/// The player's registry of UI Toolkit panels. A world has no UI Toolkit panels, so the registry is
/// always empty and an event system drives uGUI only.
/// </summary>
public static class UIElementsRuntimeUtility
{
    /// <summary>
    /// Subscribes to panel creation. No panel is ever created, so a subscription is accepted and never called.
    /// </summary>
    public static event System.Action<BaseRuntimePanel> onCreatePanel
    {
        add { }
        remove { }
    }

    /// <summary>The player panels in display order: none.</summary>
    public static List<BaseRuntimePanel> GetSortedPlayerPanels() => new();
}
