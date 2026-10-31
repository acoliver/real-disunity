namespace UnityEngine;

/// <summary>
/// The on-screen keyboard. The engine has none: <see cref="isSupported"/> is false and callers that check it
/// edit text some other way. <c>Open</c> throws <see cref="NotSupportedException"/> (the engine's choice, since
/// a caller that ignored <see cref="isSupported"/> would otherwise wait for a keyboard that never appears), so
/// no instance exists and the instance members, which the type declares for scripts to compile against,
/// throw as well. The static state is a plain stored flag or a fixed value, and nothing is shown.
/// </summary>
public class TouchScreenKeyboard
{
    private static bool _hideInput;

    /// <summary>How the keyboard last reported it ended.</summary>
    public enum Status
    {
        Visible = 0,
        Done = 1,
        Canceled = 2,
        LostFocus = 3,
    }

    private TouchScreenKeyboard() { }

    public static bool isSupported => false;

    public static bool isInPlaceEditingAllowed => false;

    /// <summary>Stored and read back; no keyboard exists to hide the input field of.</summary>
    public static bool hideInput
    {
        get => _hideInput;
        set => _hideInput = value;
    }

    public static bool visible => false;

    public static Rect area => Rect.zero;

    public static TouchScreenKeyboard Open(string text, TouchScreenKeyboardType keyboardType = TouchScreenKeyboardType.Default,
        bool autocorrection = true, bool multiline = false, bool secure = false, bool alert = false, string textPlaceholder = "",
        int characterLimit = 0) =>
        throw Unsupported("Open");

    public static TouchScreenKeyboard Open(string text, TouchScreenKeyboardType keyboardType, bool autocorrection, bool multiline,
        bool secure, bool alert, string textPlaceholder) =>
        throw Unsupported("Open");

    public string text
    {
        get => throw Unsupported(nameof(text));
        set => throw Unsupported(nameof(text));
    }

    public bool active
    {
        get => throw Unsupported(nameof(active));
        set => throw Unsupported(nameof(active));
    }

    public Status status => throw Unsupported(nameof(status));

    public int characterLimit
    {
        get => throw Unsupported(nameof(characterLimit));
        set => throw Unsupported(nameof(characterLimit));
    }

    public bool canGetSelection => throw Unsupported(nameof(canGetSelection));

    public bool canSetSelection => throw Unsupported(nameof(canSetSelection));

    public RangeInt selection
    {
        get => throw Unsupported(nameof(selection));
        set => throw Unsupported(nameof(selection));
    }

    public TouchScreenKeyboardType type => throw Unsupported(nameof(type));

    public int targetDisplay
    {
        get => throw Unsupported(nameof(targetDisplay));
        set => throw Unsupported(nameof(targetDisplay));
    }

    private static NotSupportedException Unsupported(string member) =>
        new($"TouchScreenKeyboard.{member} is not supported: the engine has no on-screen keyboard (TouchScreenKeyboard.isSupported is false)");
}
