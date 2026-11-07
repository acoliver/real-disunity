using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Building a font of a plan: <c>{"type": "Font", "name", "file", "offset", "length"}</c>, whose font file is
/// <c>length</c> bytes at <c>offset</c> of the data file <c>file</c>; a font without a file of its own has length 0.</summary>
internal static class PlayerFonts
{
    public static Font Build(JsonElement asset)
    {
        var (name, path) = (asset.Str("name"), asset.Str("file"));
        var (offset, length) = (asset.GetProperty("offset").GetInt64(), asset.GetProperty("length").GetInt64());
        if (length < 0 || length > int.MaxValue)
            throw new InvalidDataException($"resource font {name}: {length} bytes cannot be read");
        var bytes = new byte[length];
        ResourceDatabase.ReadBytes(name, path, offset, bytes);
        return new Font(name, bytes);
    }
}
