namespace UnityEngine.TextCore;

/// <summary>A rectangle of whole pixels in a font atlas texture.</summary>
[Serializable]
public struct GlyphRect : IEquatable<GlyphRect>
{
    [SerializeField] private int m_X;
    [SerializeField] private int m_Y;
    [SerializeField] private int m_Width;
    [SerializeField] private int m_Height;

    public GlyphRect(int x, int y, int width, int height)
    {
        m_X = x;
        m_Y = y;
        m_Width = width;
        m_Height = height;
    }

    /// <summary>A rectangle of the rect's members cut to whole pixels, which is the engine's choice for fractions.</summary>
    public GlyphRect(Rect rect) : this((int)rect.x, (int)rect.y, (int)rect.width, (int)rect.height) { }

    public int x { get => m_X; set => m_X = value; }
    public int y { get => m_Y; set => m_Y = value; }
    public int width { get => m_Width; set => m_Width = value; }
    public int height { get => m_Height; set => m_Height = value; }

    public static GlyphRect zero => default;

    public bool Equals(GlyphRect other) => m_X == other.m_X && m_Y == other.m_Y && m_Width == other.m_Width && m_Height == other.m_Height;
    public override bool Equals(object? obj) => obj is GlyphRect other && Equals(other);
    public override int GetHashCode() => HashCode.Combine(m_X, m_Y, m_Width, m_Height);
    public static bool operator ==(GlyphRect left, GlyphRect right) => left.Equals(right);
    public static bool operator !=(GlyphRect left, GlyphRect right) => !left.Equals(right);
}

/// <summary>The size of a glyph and how it sits on the baseline, in font units scaled by the face's point size.</summary>
[Serializable]
public struct GlyphMetrics : IEquatable<GlyphMetrics>
{
    [SerializeField] private float m_Width;
    [SerializeField] private float m_Height;
    [SerializeField] private float m_HorizontalBearingX;
    [SerializeField] private float m_HorizontalBearingY;
    [SerializeField] private float m_HorizontalAdvance;

    public GlyphMetrics(float width, float height, float bearingX, float bearingY, float advance)
    {
        m_Width = width;
        m_Height = height;
        m_HorizontalBearingX = bearingX;
        m_HorizontalBearingY = bearingY;
        m_HorizontalAdvance = advance;
    }

    public float width { get => m_Width; set => m_Width = value; }
    public float height { get => m_Height; set => m_Height = value; }
    public float horizontalBearingX { get => m_HorizontalBearingX; set => m_HorizontalBearingX = value; }
    public float horizontalBearingY { get => m_HorizontalBearingY; set => m_HorizontalBearingY = value; }
    public float horizontalAdvance { get => m_HorizontalAdvance; set => m_HorizontalAdvance = value; }

    public bool Equals(GlyphMetrics other) => m_Width == other.m_Width && m_Height == other.m_Height
        && m_HorizontalBearingX == other.m_HorizontalBearingX && m_HorizontalBearingY == other.m_HorizontalBearingY
        && m_HorizontalAdvance == other.m_HorizontalAdvance;
    public override bool Equals(object? obj) => obj is GlyphMetrics other && Equals(other);
    public override int GetHashCode() => HashCode.Combine(m_Width, m_Height, m_HorizontalBearingX, m_HorizontalBearingY, m_HorizontalAdvance);
    public static bool operator ==(GlyphMetrics left, GlyphMetrics right) => left.Equals(right);
    public static bool operator !=(GlyphMetrics left, GlyphMetrics right) => !left.Equals(right);
}

/// <summary>One glyph of a font: its index in the font, metrics and place in an atlas texture. A glyph made without data has scale 1.</summary>
[Serializable]
public class Glyph
{
    [SerializeField] private uint m_Index;
    [SerializeField] private GlyphMetrics m_Metrics;
    [SerializeField] private GlyphRect m_GlyphRect;
    [SerializeField] private float m_Scale = 1;
    [SerializeField] private int m_AtlasIndex;
    // Font assets store each glyph's class (base, ligature, mark or component) for mark and
    // ligature lookups; the engine keeps it so loaded data and copies carry it.
    [SerializeField] private GlyphClassDefinitionType m_ClassDefinitionType;

    public Glyph() { }

    public Glyph(Glyph other)
    {
        ArgumentNullException.ThrowIfNull(other);
        m_Index = other.m_Index;
        m_Metrics = other.m_Metrics;
        m_GlyphRect = other.m_GlyphRect;
        m_Scale = other.m_Scale;
        m_AtlasIndex = other.m_AtlasIndex;
        m_ClassDefinitionType = other.m_ClassDefinitionType;
    }

    public Glyph(uint index, GlyphMetrics metrics, GlyphRect glyphRect) : this(index, metrics, glyphRect, 1, 0) { }

    public Glyph(uint index, GlyphMetrics metrics, GlyphRect glyphRect, float scale, int atlasIndex)
    {
        m_Index = index;
        m_Metrics = metrics;
        m_GlyphRect = glyphRect;
        m_Scale = scale;
        m_AtlasIndex = atlasIndex;
    }

    public uint index { get => m_Index; set => m_Index = value; }
    public GlyphMetrics metrics { get => m_Metrics; set => m_Metrics = value; }
    public GlyphRect glyphRect { get => m_GlyphRect; set => m_GlyphRect = value; }
    public float scale { get => m_Scale; set => m_Scale = value; }
    public int atlasIndex { get => m_AtlasIndex; set => m_AtlasIndex = value; }
    internal GlyphClassDefinitionType classDefinitionType { get => m_ClassDefinitionType; set => m_ClassDefinitionType = value; }
}

/// <summary>A glyph's class in a font's glyph definition table.</summary>
internal enum GlyphClassDefinitionType
{
    Unknown = 0, BaseGlyph = 1, LigatureGlyph = 2, MarkGlyph = 3, ComponentGlyph = 4,
}
