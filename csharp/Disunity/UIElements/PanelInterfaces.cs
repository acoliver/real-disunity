namespace UnityEngine.UIElements;

/// <summary>
/// The data of a pointer event. A world has no UI Toolkit panels, so no pointer event is raised; the interface
/// has no members until a player calls one.
/// </summary>
public interface IPointerEvent
{
}

/// <summary>A component that owns a runtime panel. The engine creates no panels and the interface has no members until a player calls one.</summary>
public interface IRuntimePanelComponent
{
}
