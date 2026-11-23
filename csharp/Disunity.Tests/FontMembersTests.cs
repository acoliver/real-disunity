using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// A dynamic Font draws the characters asked of it into an Alpha8 atlas that is the main texture of its material, and
/// answers for each one with a CharacterInfo. The synthetic font at 20 pixels has ascent 16, line height 20 and one
/// triangle glyph, 10 by 14 pixels with its right angle at the bottom left and advance 12, for every code point 32 to 126.
/// </summary>
internal static class FontMembersTests
{
    public static void Run()
    {
        var plain = new Font("Serif", SyntheticFont.Bytes());
        WithoutAWorldAFontAnswersNothing(plain);
        DisunityHost.LoadWorld();
        try
        {
            FaceMeasurements();
            CharacterInfoMatchesTheAtlas();
            RequestsAndTheCharacterList();
            ACharacterTheFontLacksIsNotFound();
            TheMaterialCarriesTheAtlas();
            GrowthRaisesTextureRebuiltOncePerCall();
            AFontRebuildsInTheNextWorld(plain);
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: a dynamic Font measures its face, draws requested characters into its atlas and reports them as CharacterInfo");
    }

    private static Font Fresh() => new("Serif", SyntheticFont.Bytes());

    private static void WithoutAWorldAFontAnswersNothing(Font font)
    {
        Expect.That(!font.GetCharacterInfo('A', out var info, 20) && info.Equals(default(CharacterInfo)), "no character info without a world");
        Expect.That(!font.HasCharacter('A'), "no character without a world");
        Expect.Equal(0, font.ascent, "ascent without a world");
        Expect.Equal(0, font.lineHeight, "lineHeight without a world");
        Expect.That(font.material.mainTexture == null, "no atlas without a world");
    }

    private static void FaceMeasurements()
    {
        var font = Fresh();
        Expect.Equal(16, font.fontSize, "the default size");
        Expect.Equal(13, font.ascent, "ascent at 16 pixels is 12.8 rounded");
        Expect.Equal(16, font.lineHeight, "lineHeight at 16 pixels");
        Expect.Sequence(new[] { "Serif" }, font.fontNames, "fontNames");
        Expect.Throws<NotSupportedException>(() => font.fontNames = ["Other"], "fontNames cannot be set");
        var noData = new Font(7, DisunityHost.WorldHandle);
        Expect.That(!noData.GetCharacterInfo('A', out _, 20) && !noData.HasCharacter('A') && noData.ascent == 0, "a font with no file has no glyph source");
    }

    private static (Texture2D Atlas, int X, int Y) Texels(Font font, CharacterInfo info)
    {
        var atlas = (Texture2D)font.material.mainTexture!;
        return (atlas, (int)MathF.Round(info.uvBottomLeft.x * atlas.width), (int)MathF.Round(info.uvBottomLeft.y * atlas.height));
    }

    private static void CharacterInfoMatchesTheAtlas()
    {
        var font = Fresh();
        Expect.That(font.GetCharacterInfo('A', out var info, 20), "GetCharacterInfo finds 'A'");
        Expect.That(info.advance == 12 && info.glyphWidth == 10 && info.glyphHeight == 14, "advance and glyph size");
        Expect.That(info.minX == 0 && info.maxX == 10 && info.minY == 0 && info.maxY == 14 && info.index == 'A' && info.size == 20, "ink box");
        var (atlas, x, y) = Texels(font, info);
        Expect.That(atlas.format == TextureFormat.Alpha8 && atlas.width == 256 && atlas.height == 256, "a 256 by 256 Alpha8 atlas");
        Expect.Equal(new Vector2(1f / 256, 1f / 256), info.uvBottomLeft, "the glyph sits one padding texel in");
        Expect.Equal(new Vector2(11f / 256, 15f / 256), info.uvTopRight, "uvTopRight");
        var raw = atlas.GetRawTextureData();
        Expect.That(raw[y * atlas.width + x] > 200, "the bottom-left texel of the quad is inked");
        Expect.That(raw[(y + 13) * atlas.width + x + 9] == 0, "the top-right texel of the quad is empty");
        Expect.That(font.GetCharacterInfo('A', out var again, 20) && again.Equals(info), "the same character has the same info");
        Expect.That(font.GetCharacterInfo('A', out var bigger, 40) && bigger.advance == 24 && bigger.glyphHeight == 28, "another size is its own glyph");
        Expect.That(font.GetCharacterInfo('A', out var sized) && sized.size == font.fontSize, "size 0 is the font size");
        Expect.That(font.GetCharacterInfo('A', out var bold, 20, FontStyle.Bold) && bold.style == FontStyle.Bold && bold.glyphWidth == 10, "bold draws the regular face");
        Expect.That(!ReferenceEquals(atlas, null) && ReferenceEquals(atlas, font.material.mainTexture), "the atlas is kept");
    }

    private static void RequestsAndTheCharacterList()
    {
        var font = Fresh();
        Expect.That(font.characterInfo.Length == 0, "no characters before any request");
        font.RequestCharactersInTexture("AB", 20);
        font.RequestCharactersInTexture("B", 20, FontStyle.Normal);
        font.RequestCharactersInTexture("C");
        var list = font.characterInfo;
        Expect.Sequence(new[] { 'A', 'B', 'C' }, list.Select(c => (char)c.index), "characterInfo lists each requested character once");
        Expect.That(list[0].size == 20 && list[2].size == font.fontSize, "with its size");
        var atlas = (Texture2D)font.material.mainTexture!;
        Expect.That(list.All(c => c.uvBottomLeft.x >= 0 && c.uvTopRight.x <= 1 && c.uvTopRight.y <= 1) && atlas.width == 256, "UVs are inside the atlas");
        Expect.Throws<NotSupportedException>(() => font.characterInfo = list, "characterInfo cannot be set");
    }

    private static void ACharacterTheFontLacksIsNotFound()
    {
        var font = Fresh();
        Expect.That(font.HasCharacter('A') && font.HasCharacter((int)'z') && !font.HasCharacter('\u4e2d') && !font.HasCharacter(0x1F600), "HasCharacter");
        Expect.That(!font.GetCharacterInfo('\u4e2d', out var info, 20) && info.Equals(default(CharacterInfo)), "GetCharacterInfo of a missing character");
        font.RequestCharactersInTexture("\u4e2d", 20);
        Expect.That(font.characterInfo.Length == 0, "a missing character is not listed");
    }

    private static void TheMaterialCarriesTheAtlas()
    {
        var font = Fresh();
        var material = font.material;
        Expect.That(material.mainTexture == null, "no atlas before a request");
        font.GetCharacterInfo('A', out _, 20);
        Expect.That(ReferenceEquals(material, font.material) && material.mainTexture is Texture2D { width: 256 }, "the font's own material gets the atlas");
        var other = new Font("Other", SyntheticFont.Bytes());
        var replaced = new Material(Canvas.GetDefaultCanvasMaterial());
        other.material = replaced;
        other.GetCharacterInfo('A', out _, 20);
        Expect.That(replaced.mainTexture == null, "a material the game replaced keeps its own texture");
    }

    private static void GrowthRaisesTextureRebuiltOncePerCall()
    {
        var font = Fresh();
        font.GetCharacterInfo('A', out var first, 10);
        var rebuilt = new List<Font>();
        Action<Font> handler = rebuilt.Add;
        Font.textureRebuilt += handler;
        try
        {
            var calls = 0;
            for (var size = 10; size <= 120; size++, calls++) font.GetCharacterInfo('A', out _, size);
            Expect.That(rebuilt.Count is >= 1 and <= 4 && rebuilt.Count < calls, $"the atlas grew in a few rebuilds, not per glyph: {rebuilt.Count}");
            Expect.That(rebuilt.All(f => ReferenceEquals(f, font)), "the event names the font");
            var atlas = (Texture2D)font.material.mainTexture!;
            Expect.That(atlas.width > 256 && atlas.width == atlas.height, $"the atlas grew: {atlas.width}");
            Expect.That(font.GetCharacterInfo('A', out var small, 10) && small.glyphWidth == first.glyphWidth, "a small size is still there after the rebuild");
            var (_, x, y) = Texels(font, small);
            Expect.That(atlas.GetRawTextureData()[y * atlas.width + x] > 200, "and its pixels moved with it");
            var before = rebuilt.Count;
            font.GetCharacterInfo('A', out _, 10);
            Expect.Equal(before, rebuilt.Count, "a cached character raises nothing");
        }
        finally { Font.textureRebuilt -= handler; }
    }

    private static void AFontRebuildsInTheNextWorld(Font font)
    {
        var made = 0;
        Action<Font> first = _ => made++;
        Font.textureRebuilt += first;
        font.GetCharacterInfo('A', out _, 20);
        Font.textureRebuilt -= first;
        Expect.Equal(0, made, "the first atlas of a font is not a rebuild");
        DisunityHost.UnloadWorld();
        Expect.That(!font.GetCharacterInfo('A', out _, 20), "after the world unloads the font has no glyphs");
        DisunityHost.LoadWorld();
        var rebuilt = 0;
        Action<Font> handler = _ => rebuilt++;
        Font.textureRebuilt += handler;
        try
        {
            Expect.That(font.GetCharacterInfo('A', out var info, 20) && info.advance == 12, "the next world gives the font a new face");
            Expect.Equal(1, rebuilt, "and a new atlas, announced once");
        }
        finally { Font.textureRebuilt -= handler; }
    }
}
