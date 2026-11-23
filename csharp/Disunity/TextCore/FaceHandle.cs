using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine.TextCore.LowLevel;

/// <summary>
/// One face in the loaded world's face store: the world it was loaded in and its handle there. FontEngine keeps one as
/// its current face for TextMesh Pro, and a <see cref="Font"/> holds its own for UI text, so the two never share a size.
/// A face belongs to the loaded world and is gone when the world is, which every query answers as for no face.
/// </summary>
internal readonly unsafe record struct FaceHandle(ulong World, ulong Face)
{
    /// <summary>Whether the world this face was loaded in is the loaded world.</summary>
    public bool InLoadedWorld => DisunityHost.IsLoaded && World == DisunityHost.WorldHandle;

    /// <summary>Loads the face from font bytes at a point size in the loaded world; false when there is no world or the
    /// bytes are not a usable font.</summary>
    public static bool TryLoad(byte[] data, float pointSize, int faceIndex, out FaceHandle handle)
    {
        handle = default;
        if (!DisunityHost.IsLoaded) return false;
        ulong face;
        DisunityResult result;
        fixed (byte* bytes = data)
            result = NativeBindings.disunity_face_load(DisunityHost.WorldHandle, bytes, (uint)data.Length, (uint)faceIndex, pointSize, &face);
        if (result.code != 0) return false;
        handle = new FaceHandle(DisunityHost.WorldHandle, face);
        return true;
    }

    public void Unload()
    {
        if (InLoadedWorld) NativeBindings.disunity_face_unload(World, Face);
    }

    public bool SetSize(int pointSize) => InLoadedWorld && NativeBindings.disunity_face_set_size(World, Face, pointSize).code == 0;

    public bool TryInfo(out DisunityFaceInfo info)
    {
        DisunityFaceInfo native = default;
        var found = InLoadedWorld && NativeBindings.disunity_face_info(World, Face, &native).code == 0;
        info = found ? native : default;
        return found;
    }

    /// <summary>The glyph index of a code point, 0 for none (and with no face).</summary>
    public uint GlyphIndex(uint unicode)
    {
        uint glyph = 0;
        if (InLoadedWorld) NativeBindings.disunity_face_glyph_index(World, Face, unicode, &glyph);
        return glyph;
    }

    /// <summary>The name of the face: 0 for the family and 1 for the style; empty with no name.</summary>
    public string Name(uint which)
    {
        uint length;
        if (!InLoadedWorld || NativeBindings.disunity_face_name(World, Face, which, null, 0, &length).code != 0 || length == 0) return string.Empty;
        var bytes = new byte[length];
        fixed (byte* pointer = bytes)
            if (NativeBindings.disunity_face_name(World, Face, which, pointer, length, &length).code != 0) return string.Empty;
        return System.Text.Encoding.UTF8.GetString(bytes);
    }

    /// <summary>Draws the glyph into the atlas bytes at the rectangle; false when the face or glyph is missing or the draw fails.</summary>
    public bool RenderGlyph(in DisunityGlyphRender render, Span<byte> atlas)
    {
        if (!InLoadedWorld) return false;
        fixed (DisunityGlyphRender* record = &render)
        fixed (byte* pixels = atlas)
            return NativeBindings.disunity_face_glyph_render(World, Face, record, pixels, (uint)atlas.Length).code == 0;
    }

    public bool TryMetrics(uint glyphIndex, out DisunityGlyphMetrics metrics)
    {
        DisunityGlyphMetrics native = default;
        var found = InLoadedWorld && NativeBindings.disunity_face_glyph_metrics(World, Face, glyphIndex, &native).code == 0;
        metrics = found ? native : default;
        return found;
    }

    /// <summary>The nonzero pair adjustments of every ordered pair of the glyphs, in the face's current size; empty when the
    /// face, the glyphs or the adjustments are missing.</summary>
    public DisunityGlyphPair[] Pairs(uint[] glyphIndexes)
    {
        uint count = 0;
        if (!InLoadedWorld || glyphIndexes.Length == 0) return [];
        fixed (uint* glyphs = glyphIndexes)
        {
            if (NativeBindings.disunity_face_pairs(World, Face, glyphs, (uint)glyphIndexes.Length, null, &count).code != 0 || count == 0) return [];
            var native = new DisunityGlyphPair[count];
            fixed (DisunityGlyphPair* records = native)
                return NativeBindings.disunity_face_pairs(World, Face, glyphs, (uint)glyphIndexes.Length, records, &count).code == 0 ? native : [];
        }
    }
}
