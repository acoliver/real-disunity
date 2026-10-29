using UnityEngine;
using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;

/// <summary>
/// The glyph and face types a font asset stores. Each is loaded from plan JSON under the field
/// names a player stores, then read through its properties, which shows the two agree.
/// </summary>
internal static class TextCoreTests
{
    public sealed class FontData : ScriptableObject
    {
        public FaceInfo face = default;
        public List<Glyph> glyphs = new();
        public List<GlyphPairAdjustmentRecord> pairs = new();
        public MarkToBaseAdjustmentRecord markToBase = default;
        public MarkToMarkAdjustmentRecord markToMark = default;
        public LigatureSubstitutionRecord ligature = default;
    }

    private const string Face = """
        {"m_FaceIndex": "1", "m_FamilyName": "Sans", "m_StyleName": "Bold", "m_PointSize": "90", "m_Scale": "1.5",
         "m_UnitsPerEM": "2048", "m_LineHeight": "104", "m_AscentLine": "80", "m_CapLine": "70", "m_MeanLine": "50",
         "m_Baseline": "0", "m_DescentLine": "-24", "m_SuperscriptOffset": "60", "m_SuperscriptSize": "0.5",
         "m_SubscriptOffset": "-10", "m_SubscriptSize": "0.25", "m_UnderlineOffset": "-12", "m_UnderlineThickness": "6",
         "m_StrikethroughOffset": "25", "m_StrikethroughThickness": "5", "m_TabWidth": "40"}
        """;

    private const string Glyphs = """
        [{"m_Index": "36", "m_Metrics": {"m_Width": "8", "m_Height": "9", "m_HorizontalBearingX": "1",
           "m_HorizontalBearingY": "7", "m_HorizontalAdvance": "10"},
          "m_GlyphRect": {"m_X": "2", "m_Y": "3", "m_Width": "8", "m_Height": "9"}, "m_Scale": "0.5", "m_AtlasIndex": "2",
          "m_ClassDefinitionType": "3"},
         {"m_Index": "37"}]
        """;

    private const string Pairs = """
        [{"m_FirstAdjustmentRecord": {"m_GlyphIndex": "5", "m_GlyphValueRecord":
             {"m_XPlacement": "1", "m_YPlacement": "2", "m_XAdvance": "3", "m_YAdvance": "4"}},
          "m_SecondAdjustmentRecord": {"m_GlyphIndex": "6", "m_GlyphValueRecord": {"m_XAdvance": "-2"}},
          "m_FeatureLookupFlags": "4"}]
        """;

    private static string Fields() => $$$"""
        {"face": {{{Face}}}, "glyphs": {{{Glyphs}}}, "pairs": {{{Pairs}}},
         "markToBase": {"m_BaseGlyphID": "7", "m_BaseGlyphAnchorPoint": {"m_XCoordinate": "1.5", "m_YCoordinate": "2.5"},
           "m_MarkGlyphID": "8", "m_MarkPositionAdjustment": {"m_XPositionAdjustment": "3", "m_YPositionAdjustment": "4"}},
         "markToMark": {"m_BaseMarkGlyphID": "9", "m_BaseMarkGlyphAnchorPoint": {"m_XCoordinate": "5", "m_YCoordinate": "6"},
           "m_CombiningMarkGlyphID": "10", "m_CombiningMarkPositionAdjustment": {"m_XPositionAdjustment": "7", "m_YPositionAdjustment": "8"}},
         "ligature": {"m_ComponentGlyphIDs": ["70", "71"], "m_LigatureGlyphID": "72"}}
        """;

    public static void Run()
    {
        var asset = new PlanHarness.Asset(typeof(FontData), "font", Fields());
        PlanHarness.With([asset], () =>
        {
            var data = Resources.Load<FontData>(PlanHarness.PathOf(asset)) ?? throw new InvalidOperationException("font data did not load");
            LoadsFaceInfo(data.face);
            LoadsGlyphs(data.glyphs);
            LoadsAdjustmentRecords(data);
        });
        ComparesAndAdds();
        Console.WriteLine("PASS: TextCore face and glyph types load under their stored field names and compare");
    }

    private static void LoadsFaceInfo(FaceInfo face)
    {
        Expect.That(face is { faceIndex: 1, familyName: "Sans", styleName: "Bold", pointSize: 90, unitsPerEM: 2048 }, "face identity fields");
        Expect.That(face.scale == 1.5f && face.lineHeight == 104 && face.ascentLine == 80 && face.capLine == 70 && face.meanLine == 50
            && face.baseline == 0 && face.descentLine == -24, "face vertical metrics");
        Expect.That(face.superscriptOffset == 60 && face.superscriptSize == 0.5f && face.subscriptOffset == -10 && face.subscriptSize == 0.25f,
            "face script offsets");
        Expect.That(face.underlineOffset == -12 && face.underlineThickness == 6 && face.strikethroughOffset == 25
            && face.strikethroughThickness == 5 && face.tabWidth == 40, "face line decorations and tab width");
        var changed = face;
        changed.familyName = "Serif";
        changed.tabWidth = 12;
        Expect.That(changed.familyName == "Serif" && changed.tabWidth == 12 && face.familyName == "Sans", "face properties set their own copy");
    }

    private static void LoadsGlyphs(List<Glyph> glyphs)
    {
        Expect.Equal(2, glyphs.Count, "glyph count");
        var first = glyphs[0];
        Expect.That(first.index == 36 && first.scale == 0.5f && first.atlasIndex == 2, "glyph scalars");
        Expect.That(first.metrics == new GlyphMetrics(8, 9, 1, 7, 10), "glyph metrics");
        Expect.That(first.glyphRect == new GlyphRect(2, 3, 8, 9), "glyph rect");
        Expect.That(glyphs[1] is { index: 37, scale: 1, atlasIndex: 0 }, "a glyph's omitted fields keep their defaults, scale 1");
        Expect.Equal(3, GlyphClass(first), "a glyph keeps the class its font stores");
        Expect.Equal(3, GlyphClass(new Glyph(first)), "a copied glyph keeps its class");
        Expect.Equal(0, GlyphClass(glyphs[1]), "a glyph without a stored class is of unknown class");
    }

    // The class is engine state a game does not read, so the test reads the stored field.
    private static int GlyphClass(Glyph glyph) => System.Convert.ToInt32(typeof(Glyph)
        .GetField("m_ClassDefinitionType", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic)!
        .GetValue(glyph));

    private static void LoadsAdjustmentRecords(FontData data)
    {
        var pair = data.pairs.Single();
        Expect.That(pair.firstAdjustmentRecord.glyphIndex == 5 && pair.secondAdjustmentRecord.glyphIndex == 6, "pair glyph indexes");
        Expect.That(pair.firstAdjustmentRecord.glyphValueRecord == new GlyphValueRecord(1, 2, 3, 4), "pair first value record");
        Expect.That(pair.secondAdjustmentRecord.glyphValueRecord == new GlyphValueRecord(0, 0, -2, 0), "pair second value record");
        Expect.Equal(FontFeatureLookupFlags.IgnoreLigatures, pair.featureLookupFlags, "pair feature flags");
        var toBase = data.markToBase;
        Expect.That(toBase.baseGlyphID == 7 && toBase.markGlyphID == 8, "mark to base glyphs");
        Expect.That(toBase.baseGlyphAnchorPoint is { xCoordinate: 1.5f, yCoordinate: 2.5f }
            && toBase.markPositionAdjustment is { xPositionAdjustment: 3, yPositionAdjustment: 4 }, "mark to base positions");
        var toMark = data.markToMark;
        Expect.That(toMark.baseMarkGlyphID == 9 && toMark.combiningMarkGlyphID == 10, "mark to mark glyphs");
        Expect.That(toMark.baseMarkGlyphAnchorPoint is { xCoordinate: 5, yCoordinate: 6 }
            && toMark.combiningMarkPositionAdjustment is { xPositionAdjustment: 7, yPositionAdjustment: 8 }, "mark to mark positions");
        Expect.Sequence(new uint[] { 70, 71 }, data.ligature.componentGlyphIDs, "ligature components");
        Expect.Equal(72u, data.ligature.ligatureGlyphID, "ligature glyph");
    }

    private static void ComparesAndAdds()
    {
        Expect.That(new GlyphRect(1, 2, 3, 4) == new GlyphRect(1, 2, 3, 4) && new GlyphRect(1, 2, 3, 4) != new GlyphRect(1, 2, 3, 5),
            "GlyphRect equality compares all four members");
        Expect.That(new GlyphRect(1, 2, 3, 4).Equals((object)new GlyphRect(1, 2, 3, 4)) && !new GlyphRect(1, 2, 3, 4).Equals("x")
            && new GlyphRect(1, 2, 3, 4).GetHashCode() == new GlyphRect(1, 2, 3, 4).GetHashCode(), "GlyphRect Equals and hash");
        Expect.That(GlyphRect.zero == new GlyphRect(0, 0, 0, 0), "GlyphRect.zero is empty");
        Expect.That(new GlyphRect(new Rect(1.9f, 2.2f, 3.5f, 4.6f)) == new GlyphRect(1, 2, 3, 4), "a GlyphRect from a Rect truncates to whole pixels");
        Expect.That(new GlyphMetrics(1, 2, 3, 4, 5) != new GlyphMetrics(1, 2, 3, 4, 6) && new GlyphMetrics(1, 2, 3, 4, 5) == new GlyphMetrics(1, 2, 3, 4, 5),
            "GlyphMetrics equality compares the advance too");
        var sum = new GlyphValueRecord(1, 2, 3, 4) + new GlyphValueRecord(10, 20, 30, 40);
        Expect.That(sum == new GlyphValueRecord(11, 22, 33, 44) && sum != new GlyphValueRecord(11, 22, 33, 45), "GlyphValueRecord addition adds each member");
        var source = new Glyph(9, new GlyphMetrics(1, 2, 3, 4, 5), new GlyphRect(6, 7, 8, 9), 2, 3);
        var copy = new Glyph(source);
        Expect.That(copy.index == 9 && copy.metrics == source.metrics && copy.glyphRect == source.glyphRect && copy.scale == 2 && copy.atlasIndex == 3,
            "the copy constructor copies every member");
        copy.index = 10;
        Expect.That(source.index == 9, "a copied glyph is independent");
        var simple = new Glyph(11, new GlyphMetrics(1, 1, 1, 1, 1), GlyphRect.zero);
        Expect.That(simple.scale == 1 && simple.atlasIndex == 0, "the short constructor sets scale 1 and atlas 0");
    }
}
