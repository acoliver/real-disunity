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

    /// <summary>
    /// Tells the registry which event system feeds UI Toolkit panels. With no panels there is no input to
    /// route, so registration only checks its argument.
    /// </summary>
    public static void RegisterEventSystem(Object eventSystem) => RequireEventSystem(eventSystem);

    public static void UnregisterEventSystem(Object eventSystem) => RequireEventSystem(eventSystem);

    private static void RequireEventSystem(Object eventSystem) => ArgumentNullException.ThrowIfNull(eventSystem);

    /// <summary>The player panels in display order: none.</summary>
    public static List<BaseRuntimePanel> GetSortedPlayerPanels() => new();
}
