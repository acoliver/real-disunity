using System.Buffers.Binary;
using System.Text;

// Original synthetic triangle outlines and metrics, with no external font data.
internal static class SyntheticFont
{
    private static void U16(byte[] bytes, int at, int value) => BinaryPrimitives.WriteUInt16BigEndian(bytes.AsSpan(at), unchecked((ushort)value));
    private static void U32(byte[] bytes, int at, uint value) => BinaryPrimitives.WriteUInt32BigEndian(bytes.AsSpan(at), value);
    private static byte[] Words(params int[] words)
    {
        var result = new byte[words.Length * 2];
        for (var i = 0; i < words.Length; i++) U16(result, i * 2, words[i]);
        return result;
    }
    public static byte[] Bytes()
    {
        var head = new byte[54];
        U32(head, 0, 0x10000); U32(head, 12, 0x5f0f3cf5); U16(head, 18, 1000);
        var hhea = new byte[36];
        U32(hhea, 0, 0x10000); U16(hhea, 4, 800); U16(hhea, 6, -200); U16(hhea, 34, 2);
        var maxp = new byte[32];
        U32(maxp, 0, 0x10000); U16(maxp, 4, 2);
        var glyph = Words(1, 0, 0, 500, 700, 2, 0).Concat(new byte[] { 1, 1, 1 })
            .Concat(Words(0, 500, -500, 0, 0, 700)).Append((byte)0).ToArray();
        (string Tag, byte[] Data)[] tables = [
            ("cmap", Cmap()), ("glyf", glyph), ("head", head), ("hhea", hhea),
            ("hmtx", Words(500, 0, 600, 0)), ("loca", Words(0, 0, glyph.Length / 2)), ("maxp", maxp),
        ];
        var output = new List<byte>(new byte[12 + tables.Length * 16]);
        var header = output.ToArray();
        U32(header, 0, 0x10000); U16(header, 4, tables.Length);
        for (var i = 0; i < tables.Length; i++)
        {
            var at = 12 + i * 16;
            Encoding.ASCII.GetBytes(tables[i].Tag).CopyTo(header, at);
            U32(header, at + 8, checked((uint)output.Count));
            U32(header, at + 12, checked((uint)tables[i].Data.Length));
            output.AddRange(tables[i].Data);
            while (output.Count % 4 != 0) output.Add(0);
        }
        for (var i = 0; i < header.Length; i++) output[i] = header[i];
        return output.ToArray();
    }
    private static byte[] Cmap()
    {
        var result = Words(0, 1, 3, 1).Concat(new byte[] { 0, 0, 0, 12 }).ToList();
        result.AddRange(Words(4, 16 + 96 * 8, 0, 96 * 2, 128, 6, 96 * 2 - 128));
        for (var c = 32; c <= 126; c++) result.AddRange(Words(c));
        result.AddRange(Words(65535, 0));
        for (var c = 32; c <= 126; c++) result.AddRange(Words(c));
        result.AddRange(Words(65535));
        for (var c = 32; c <= 126; c++) result.AddRange(Words(1 - c));
        result.AddRange(Words(1));
        result.AddRange(new byte[96 * 2]);
        return result.ToArray();
    }
}
