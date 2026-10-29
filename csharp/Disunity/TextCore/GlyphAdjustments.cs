namespace UnityEngine.TextCore.LowLevel;

/// <summary>A change to a glyph's placement and advance. Adding two records adds each member.</summary>
[Serializable]
public struct GlyphValueRecord : IEquatable<GlyphValueRecord>
{
    [SerializeField] private float m_XPlacement;
    [SerializeField] private float m_YPlacement;
    [SerializeField] private float m_XAdvance;
    [SerializeField] private float m_YAdvance;

    public GlyphValueRecord(float xPlacement, float yPlacement, float xAdvance, float yAdvance)
    {
        m_XPlacement = xPlacement;
        m_YPlacement = yPlacement;
        m_XAdvance = xAdvance;
        m_YAdvance = yAdvance;
    }

    public float xPlacement { get => m_XPlacement; set => m_XPlacement = value; }
    public float yPlacement { get => m_YPlacement; set => m_YPlacement = value; }
    public float xAdvance { get => m_XAdvance; set => m_XAdvance = value; }
    public float yAdvance { get => m_YAdvance; set => m_YAdvance = value; }

    public static GlyphValueRecord operator +(GlyphValueRecord a, GlyphValueRecord b) =>
        new(a.m_XPlacement + b.m_XPlacement, a.m_YPlacement + b.m_YPlacement, a.m_XAdvance + b.m_XAdvance, a.m_YAdvance + b.m_YAdvance);

    public bool Equals(GlyphValueRecord other) => m_XPlacement == other.m_XPlacement && m_YPlacement == other.m_YPlacement
        && m_XAdvance == other.m_XAdvance && m_YAdvance == other.m_YAdvance;
    public override bool Equals(object? obj) => obj is GlyphValueRecord other && Equals(other);
    public override int GetHashCode() => HashCode.Combine(m_XPlacement, m_YPlacement, m_XAdvance, m_YAdvance);
    public static bool operator ==(GlyphValueRecord left, GlyphValueRecord right) => left.Equals(right);
    public static bool operator !=(GlyphValueRecord left, GlyphValueRecord right) => !left.Equals(right);
}

/// <summary>The adjustment of one glyph in a pair.</summary>
[Serializable]
public struct GlyphAdjustmentRecord
{
    [SerializeField] private uint m_GlyphIndex;
    [SerializeField] private GlyphValueRecord m_GlyphValueRecord;

    public GlyphAdjustmentRecord(uint glyphIndex, GlyphValueRecord glyphValueRecord)
    {
        m_GlyphIndex = glyphIndex;
        m_GlyphValueRecord = glyphValueRecord;
    }

    public uint glyphIndex { get => m_GlyphIndex; set => m_GlyphIndex = value; }
    public GlyphValueRecord glyphValueRecord { get => m_GlyphValueRecord; set => m_GlyphValueRecord = value; }
}

/// <summary>The kerning of two adjacent glyphs. A pair made from two records has no lookup flags.</summary>
[Serializable]
public struct GlyphPairAdjustmentRecord
{
    [SerializeField] private GlyphAdjustmentRecord m_FirstAdjustmentRecord;
    [SerializeField] private GlyphAdjustmentRecord m_SecondAdjustmentRecord;
    [SerializeField] private FontFeatureLookupFlags m_FeatureLookupFlags;

    public GlyphPairAdjustmentRecord(GlyphAdjustmentRecord firstAdjustmentRecord, GlyphAdjustmentRecord secondAdjustmentRecord)
    {
        m_FirstAdjustmentRecord = firstAdjustmentRecord;
        m_SecondAdjustmentRecord = secondAdjustmentRecord;
        m_FeatureLookupFlags = FontFeatureLookupFlags.None;
    }

    public GlyphAdjustmentRecord firstAdjustmentRecord { get => m_FirstAdjustmentRecord; set => m_FirstAdjustmentRecord = value; }
    public GlyphAdjustmentRecord secondAdjustmentRecord { get => m_SecondAdjustmentRecord; set => m_SecondAdjustmentRecord = value; }
    public FontFeatureLookupFlags featureLookupFlags { get => m_FeatureLookupFlags; set => m_FeatureLookupFlags = value; }
}

/// <summary>A point on a glyph a mark attaches to.</summary>
[Serializable]
public struct GlyphAnchorPoint
{
    [SerializeField] private float m_XCoordinate;
    [SerializeField] private float m_YCoordinate;

    public float xCoordinate { get => m_XCoordinate; set => m_XCoordinate = value; }
    public float yCoordinate { get => m_YCoordinate; set => m_YCoordinate = value; }
}

/// <summary>The offset of a mark from its anchor point.</summary>
[Serializable]
public struct MarkPositionAdjustment
{
    [SerializeField] private float m_XPositionAdjustment;
    [SerializeField] private float m_YPositionAdjustment;

    public MarkPositionAdjustment(float x, float y)
    {
        m_XPositionAdjustment = x;
        m_YPositionAdjustment = y;
    }

    public float xPositionAdjustment { get => m_XPositionAdjustment; set => m_XPositionAdjustment = value; }
    public float yPositionAdjustment { get => m_YPositionAdjustment; set => m_YPositionAdjustment = value; }
}

/// <summary>Where a mark glyph sits on a base glyph.</summary>
[Serializable]
public struct MarkToBaseAdjustmentRecord
{
    [SerializeField] private uint m_BaseGlyphID;
    [SerializeField] private GlyphAnchorPoint m_BaseGlyphAnchorPoint;
    [SerializeField] private uint m_MarkGlyphID;
    [SerializeField] private MarkPositionAdjustment m_MarkPositionAdjustment;

    public uint baseGlyphID { get => m_BaseGlyphID; set => m_BaseGlyphID = value; }
    public GlyphAnchorPoint baseGlyphAnchorPoint { get => m_BaseGlyphAnchorPoint; set => m_BaseGlyphAnchorPoint = value; }
    public uint markGlyphID { get => m_MarkGlyphID; set => m_MarkGlyphID = value; }
    public MarkPositionAdjustment markPositionAdjustment { get => m_MarkPositionAdjustment; set => m_MarkPositionAdjustment = value; }
}

/// <summary>Where a combining mark glyph sits on another mark.</summary>
[Serializable]
public struct MarkToMarkAdjustmentRecord
{
    [SerializeField] private uint m_BaseMarkGlyphID;
    [SerializeField] private GlyphAnchorPoint m_BaseMarkGlyphAnchorPoint;
    [SerializeField] private uint m_CombiningMarkGlyphID;
    [SerializeField] private MarkPositionAdjustment m_CombiningMarkPositionAdjustment;

    public uint baseMarkGlyphID { get => m_BaseMarkGlyphID; set => m_BaseMarkGlyphID = value; }
    public GlyphAnchorPoint baseMarkGlyphAnchorPoint { get => m_BaseMarkGlyphAnchorPoint; set => m_BaseMarkGlyphAnchorPoint = value; }
    public uint combiningMarkGlyphID { get => m_CombiningMarkGlyphID; set => m_CombiningMarkGlyphID = value; }
    public MarkPositionAdjustment combiningMarkPositionAdjustment { get => m_CombiningMarkPositionAdjustment; set => m_CombiningMarkPositionAdjustment = value; }
}

/// <summary>The glyphs that combine into one ligature glyph.</summary>
[Serializable]
public struct LigatureSubstitutionRecord
{
    [SerializeField] private uint[] m_ComponentGlyphIDs;
    [SerializeField] private uint m_LigatureGlyphID;

    public uint[] componentGlyphIDs { get => m_ComponentGlyphIDs; set => m_ComponentGlyphIDs = value; }
    public uint ligatureGlyphID { get => m_LigatureGlyphID; set => m_LigatureGlyphID = value; }
}
