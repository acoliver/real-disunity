namespace UnityEngine.UIElements;

/// <summary>
/// How UI Toolkit panels receive input. A world has no UI Toolkit panels, so the engine reports no current
/// configuration and event systems drive uGUI only.
/// </summary>
public class PanelInputConfiguration : MonoBehaviour
{
    public enum PanelInputRedirection
    {
        AutoSwitch = 0, Never = 1, Always = 2,
    }

    [System.Serializable]
    public struct Settings
    {
        public PanelInputRedirection panelInputRedirection;
        public bool processWorldSpaceInput;
        public LayerMask interactionLayers;
        public float maxInteractionDistance;
        public bool autoCreatePanelComponents;

        public static Settings Default => new()
        {
            panelInputRedirection = PanelInputRedirection.AutoSwitch,
            processWorldSpaceInput = true,
            interactionLayers = ~0,
            maxInteractionDistance = float.PositiveInfinity,
            autoCreatePanelComponents = true,
        };
    }

    /// <summary>Raised when a configuration's settings are applied. The engine never raises it.</summary>
    public static System.Action<PanelInputConfiguration>? onApply;

    private Settings _settings = Settings.Default;

    /// <summary>The configuration of the panels in play. No world has UI Toolkit panels, so this is null.</summary>
    public static PanelInputConfiguration? current => null;

    public Settings settings
    {
        get => _settings;
        set
        {
            if (!System.Enum.IsDefined(value.panelInputRedirection))
                throw new System.ArgumentOutOfRangeException(nameof(value), value.panelInputRedirection, "unknown panel input redirection");
            _settings = value;
        }
    }
}
