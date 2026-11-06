namespace UnityEngine;

/// <summary>
/// The text, image and tooltip of an interface element. Plain data: the engine has no immediate-mode interface, so nothing
/// draws it, and a pipeline's debug tools only hold values of this type. A null text or tooltip becomes empty.
/// <see cref="none"/> is one shared empty value, and <see cref="Temp(string)"/> and its forms return a scratch value that
/// the next call to any of them replaces.
/// </summary>
[Serializable]
public class GUIContent
{
    private static readonly GUIContent None = new("");
    private static readonly GUIContent Scratch = new();

    private string _text = "";
    private string _tooltip = "";

    public GUIContent() { }

    public GUIContent(string? text) => _text = text ?? "";

    public GUIContent(Texture? image) => this.image = image;

    public GUIContent(string? text, Texture? image)
    {
        _text = text ?? "";
        this.image = image;
    }

    public GUIContent(string? text, string? tooltip)
    {
        _text = text ?? "";
        _tooltip = tooltip ?? "";
    }

    public GUIContent(Texture? image, string? tooltip)
    {
        this.image = image;
        _tooltip = tooltip ?? "";
    }

    public GUIContent(string? text, Texture? image, string? tooltip)
    {
        _text = text ?? "";
        this.image = image;
        _tooltip = tooltip ?? "";
    }

    public GUIContent(GUIContent src)
    {
        ArgumentNullException.ThrowIfNull(src);
        (_text, image, _tooltip) = (src._text, src.image, src._tooltip);
    }

    public string text { get => _text; set => _text = value ?? ""; }
    public Texture? image { get; set; }
    public string tooltip { get => _tooltip; set => _tooltip = value ?? ""; }

    public static GUIContent none => None;

    public static GUIContent Temp(string? t) => Fill(t, null, null);
    public static GUIContent Temp(Texture? i) => Fill(null, i, null);
    public static GUIContent Temp(string? t, Texture? i) => Fill(t, i, null);
    public static GUIContent Temp(string? t, string? tip) => Fill(t, null, tip);

    /// <summary>Does nothing: the scratch value holds no reference that outlives a call.</summary>
    public static void ClearStaticCache() { }

    private static GUIContent Fill(string? t, Texture? i, string? tip)
    {
        Scratch._text = t ?? "";
        Scratch.image = i;
        Scratch._tooltip = tip ?? "";
        return Scratch;
    }

    public override string ToString() => _text;
}
