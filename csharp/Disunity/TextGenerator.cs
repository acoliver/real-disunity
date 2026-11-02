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
/// Lays text out into vertices, character positions and lines, which UI Text turns into a mesh. The engine's choice:
/// the generator lays out no text yet. Empty text gives an empty result and any other text is rejected with a
/// <see cref="NotSupportedException"/> naming the member, so a UI Text with text reports that instead of drawing nothing.
/// </summary>
public sealed class TextGenerator
{
    private static readonly UIVertex[] NoVerts = [];
    private static readonly UICharInfo[] NoCharacters = [];
    private static readonly UILineInfo[] NoLines = [];

    public TextGenerator() { }

    public TextGenerator(int initialCapacity)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(initialCapacity);
    }

    public IList<UIVertex> verts => NoVerts;
    public IList<UICharInfo> characters => NoCharacters;
    public IList<UILineInfo> lines => NoLines;
    public Rect rectExtents => default;
    public int vertexCount => 0;
    public int characterCount => 0;
    public int characterCountVisible => 0;
    public int lineCount => 0;
    public int fontSizeUsedForBestFit => 0;

    public UIVertex[] GetVerticesArray() => [];
    public UICharInfo[] GetCharactersArray() => [];
    public UILineInfo[] GetLinesArray() => [];

    /// <summary>Clears the result. The generator keeps no other state, so there is nothing to forget.</summary>
    public void Invalidate() { }

    public bool Populate(string str, TextGenerationSettings settings)
    {
        RequireNoLayout(str, "TextGenerator.Populate");
        return true;
    }

    public bool PopulateWithErrors(string str, TextGenerationSettings settings, GameObject context)
    {
        RequireNoLayout(str, "TextGenerator.PopulateWithErrors");
        return true;
    }

    public float GetPreferredWidth(string str, TextGenerationSettings settings)
    {
        RequireNoLayout(str, "TextGenerator.GetPreferredWidth");
        return 0f;
    }

    public float GetPreferredHeight(string str, TextGenerationSettings settings)
    {
        RequireNoLayout(str, "TextGenerator.GetPreferredHeight");
        return 0f;
    }

    private static void RequireNoLayout(string str, string member)
    {
        ArgumentNullException.ThrowIfNull(str);
        if (str.Length != 0) throw new NotSupportedException($"{member} cannot lay out text: the engine has no layout for a Font asset");
    }
}
