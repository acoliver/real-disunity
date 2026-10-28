using Disunity.Hosting;

namespace UnityEngine;

/// <summary>One finger on a touch screen. The desktop host reports no touches, so a value only comes from a script.</summary>
public struct Touch
{
    public int fingerId { get; set; }
    public Vector2 position { get; set; }
    public Vector2 rawPosition { get; set; }
    public Vector2 deltaPosition { get; set; }
    public float deltaTime { get; set; }
    public int tapCount { get; set; }
    public TouchPhase phase { get; set; }
    public float pressure { get; set; }
    public float maximumPossiblePressure { get; set; }
    public TouchType type { get; set; }
    public float altitudeAngle { get; set; }
    public float azimuthAngle { get; set; }
    public float radius { get; set; }
    public float radiusVariance { get; set; }
}

/// <summary>
/// Buttons, the mouse, touches and text composition. The host delivers keys only, so the mouse is present but idle (position
/// zero, no buttons, no scroll) and no touch screen exists. Buttons read the keys of the registered input axes.
/// </summary>
public static partial class Input
{
    private static readonly KeyCode[] AllKeys = Enum.GetValues<KeyCode>().Where(key => key != KeyCode.None).ToArray();

    private static InputAxisDefinition Axis(string buttonName)
    {
        ArgumentNullException.ThrowIfNull(buttonName);
        return DisunityHost.AxisDefinitions.TryGetValue(buttonName, out var axis)
            ? axis
            : throw new ArgumentException($"Input button '{buttonName}' is not registered", nameof(buttonName));
    }

    private static IEnumerable<KeyCode> Keys(InputAxisDefinition axis) =>
        new[] { axis.Negative, axis.Positive, axis.AltNegative, axis.AltPositive }.Where(key => key != KeyCode.None);

    /// <summary>True while any key of the named input axis (negative, positive or either alternative) is held.</summary>
    public static bool GetButton(string buttonName) => Keys(Axis(buttonName)).Any(GetKey);
    public static bool GetButtonDown(string buttonName) => Keys(Axis(buttonName)).Any(GetKeyDown);
    public static bool GetButtonUp(string buttonName) => Keys(Axis(buttonName)).Any(GetKeyUp);

    public static bool anyKey => AllKeys.Any(GetKey);
    public static bool anyKeyDown => AllKeys.Any(GetKeyDown);

    public static bool mousePresent => true;
    public static Vector3 mousePosition => Vector3.zero;
    public static Vector2 mouseScrollDelta => Vector2.zero;

    private static void CheckButton(int button)
    {
        if (button < 0) throw new ArgumentOutOfRangeException(nameof(button), "A mouse button is 0 or more");
    }

    public static bool GetMouseButton(int button) { CheckButton(button); return false; }
    public static bool GetMouseButtonDown(int button) { CheckButton(button); return false; }
    public static bool GetMouseButtonUp(int button) { CheckButton(button); return false; }

    public static bool touchSupported => false;
    public static int touchCount => 0;
    public static Touch[] touches => [];

    public static Touch GetTouch(int index) =>
        throw new ArgumentOutOfRangeException(nameof(index), $"Touch {index} does not exist: the desktop host reports no touches");

    public static string compositionString => "";
    public static IMECompositionMode imeCompositionMode { get; set; } = IMECompositionMode.Auto;
    public static Vector2 compositionCursorPos { get; set; }
}
