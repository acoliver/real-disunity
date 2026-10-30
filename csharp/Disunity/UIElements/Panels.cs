namespace UnityEngine.UIElements;

/// <summary>
/// A UI Toolkit panel. A world has no UI Toolkit panels, so the engine never creates one; the type exists
/// so that packages that mention it load.
/// </summary>
public abstract class BaseVisualElementPanel
{
}

/// <summary>A panel that draws into a player. The engine creates none.</summary>
public abstract class BaseRuntimePanel : BaseVisualElementPanel
{
}
