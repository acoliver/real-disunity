namespace UnityEngine;

/// <summary>One laid-out character: where its cursor starts and how wide it is.</summary>
public struct UICharInfo
{
    public Vector2 cursorPos;
    public float charWidth;
}

/// <summary>One laid-out line: the index of its first character, its height and its top edge.</summary>
public struct UILineInfo
{
    public int startCharIdx;
    public int height;
    public float topY;
    public float leading;
}

/// <summary>Everything a <see cref="TextGenerator"/> needs to lay out one string. Two settings are equal when every field is.</summary>
public struct TextGenerationSettings : IEquatable<TextGenerationSettings>
{
    public Font font;
    public Color color;
    public int fontSize;
    public float lineSpacing;
    public bool richText;
    public float scaleFactor;
    public FontStyle fontStyle;
    public TextAnchor textAnchor;
    public bool alignByGeometry;
    public bool resizeTextForBestFit;
    public int resizeTextMinSize;
    public int resizeTextMaxSize;
    public bool updateBounds;
    public VerticalWrapMode verticalOverflow;
    public HorizontalWrapMode horizontalOverflow;
    public Vector2 generationExtents;
    public Vector2 pivot;
    public bool generateOutOfBounds;

    public readonly bool Equals(TextGenerationSettings other) =>
        ReferenceEquals(font, other.font) && color == other.color && fontSize == other.fontSize
        && lineSpacing == other.lineSpacing && richText == other.richText && scaleFactor == other.scaleFactor
        && fontStyle == other.fontStyle && textAnchor == other.textAnchor && alignByGeometry == other.alignByGeometry
        && resizeTextForBestFit == other.resizeTextForBestFit && resizeTextMinSize == other.resizeTextMinSize
        && resizeTextMaxSize == other.resizeTextMaxSize && updateBounds == other.updateBounds
        && verticalOverflow == other.verticalOverflow && horizontalOverflow == other.horizontalOverflow
        && generationExtents == other.generationExtents && pivot == other.pivot
        && generateOutOfBounds == other.generateOutOfBounds;

    public override readonly bool Equals(object? obj) => obj is TextGenerationSettings other && Equals(other);

    public override readonly int GetHashCode()
    {
        var hash = new HashCode();
        hash.Add(font);
        hash.Add(fontSize);
        hash.Add(lineSpacing);
        hash.Add(scaleFactor);
        hash.Add(textAnchor);
        hash.Add(generationExtents);
        hash.Add(pivot);
        return hash.ToHashCode();
    }
}

/// <summary>
/// Lays text out into vertices, character positions and lines, which UI Text turns into a mesh. The text is laid out
/// from the glyph metrics of the settings' font, whose atlas the glyphs are drawn into, at the settings' size and scale
/// factor, in pixels measured from the pivot with y growing upward. Each glyph with ink is four vertices (top left, top
/// right, bottom right, bottom left) whose UVs refer to the atlas that is the font material's main texture. A result is
/// kept until the text, the settings or the font's atlas change. With rich text on the tags b, i, size and color are read; bold
/// and italic draw the regular face. Best fit picks the largest size in its range at which the wrapped text fits the
/// extents, and aligning by geometry aligns the ink of each line.
/// </summary>
public sealed class TextGenerator
{
    private readonly List<UIVertex> _verts;
    private readonly List<UICharInfo> _characters = [];
    private readonly List<UILineInfo> _lines = [];
    private readonly IList<UIVertex> _vertsView;
    private readonly IList<UICharInfo> _charactersView;
    private readonly IList<UILineInfo> _linesView;
    private Rect _extents;
    private int _visible;
    private int _bestFitSize;
    private string? _cachedText;
    private TextGenerationSettings _cachedSettings;
    private int _cachedGeneration;

    public TextGenerator() : this(0) { }

    public TextGenerator(int initialCapacity)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(initialCapacity);
        _verts = new List<UIVertex>(initialCapacity);
        _vertsView = _verts.AsReadOnly();
        _charactersView = _characters.AsReadOnly();
        _linesView = _lines.AsReadOnly();
    }

    public IList<UIVertex> verts => _vertsView;
    public IList<UICharInfo> characters => _charactersView;
    public IList<UILineInfo> lines => _linesView;
    public Rect rectExtents => _extents;
    public int vertexCount => _verts.Count;
    public int characterCount => _characters.Count;
    public int characterCountVisible => _visible;
    public int lineCount => _lines.Count;
    public int fontSizeUsedForBestFit => _bestFitSize;

    public UIVertex[] GetVerticesArray() => _verts.ToArray();
    public UICharInfo[] GetCharactersArray() => _characters.ToArray();
    public UILineInfo[] GetLinesArray() => _lines.ToArray();

    /// <summary>Clears the result, so the next populate lays the text out again.</summary>
    public void Invalidate()
    {
        _verts.Clear();
        _characters.Clear();
        _lines.Clear();
        (_extents, _visible, _bestFitSize, _cachedText) = (default, 0, 0, null);
    }

    /// <summary>Lays the text out. False, with an empty result, when the settings have no font or the engine cannot draw it.</summary>
    public bool Populate(string str, TextGenerationSettings settings) => Generate(str, settings, false, null);

    /// <summary>Like <see cref="Populate"/>, and logs the reason a font cannot be used, once per font, against the context.</summary>
    public bool PopulateWithErrors(string str, TextGenerationSettings settings, GameObject context) => Generate(str, settings, true, context);

    private bool Generate(string str, TextGenerationSettings settings, bool report, GameObject? context)
    {
        ArgumentNullException.ThrowIfNull(str);
        if (str.Length == 0)
        {
            Invalidate();
            return true;
        }
        if (_cachedText == str && _cachedSettings.Equals(settings) && settings.font?.Glyphs is { } glyphs && glyphs.TryFace(out _)
            && _cachedGeneration == glyphs.AtlasGeneration) return true;
        Invalidate();
        if (TextLayoutJob.For(str, settings) is not { } job)
        {
            if (report) ReportUnusableFont(settings.font, context);
            return false;
        }
        Store(job.Layout());
        (_cachedText, _cachedSettings, _cachedGeneration) = (str, settings, job.AtlasGeneration);
        job.Font.FlushTextureRebuilt();
        return true;
    }

    private void Store(TextLayoutResult result)
    {
        _verts.AddRange(result.Verts);
        _characters.AddRange(result.Characters);
        _lines.AddRange(result.Lines);
        (_extents, _visible, _bestFitSize) = (result.Extents, result.Visible, result.FontSize);
    }

    private static void ReportUnusableFont(Font? font, GameObject? context)
    {
        if (font is null) Debug.LogError("TextGenerator.PopulateWithErrors: the settings have no font", context);
        else if (font.ReportLayoutFailureOnce()) Debug.LogError($"TextGenerator.PopulateWithErrors: font '{font.name}' has no font file the engine can draw", context);
    }

    /// <summary>The width of the widest line when the text does not wrap, in pixels at the settings' scale; 0 for no text or
    /// no usable font. The generator's result is left as it was.</summary>
    public float GetPreferredWidth(string str, TextGenerationSettings settings)
    {
        ArgumentNullException.ThrowIfNull(str);
        return str.Length == 0 || TextLayoutJob.For(str, settings) is not { } job ? 0f : job.PreferredWidth();
    }

    /// <summary>The height of the text wrapped at the settings' width, in pixels at the settings' scale; 0 for no text or
    /// no usable font. The generator's result is left as it was.</summary>
    public float GetPreferredHeight(string str, TextGenerationSettings settings)
    {
        ArgumentNullException.ThrowIfNull(str);
        return str.Length == 0 || TextLayoutJob.For(str, settings) is not { } job ? 0f : job.PreferredHeight();
    }
}
