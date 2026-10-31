using System.Text;
using Disunity.Hosting;

namespace UnityEngine;

public partial class Material
{
    /// <summary>
    /// A 32-bit CRC (the CRC-32 of zlib) over the shader name, the render queue set on the material (-1 when it comes from the shader, which the shader name covers), the enabled
    /// keywords in sorted order, the override tags by name and every property that was set, as its kind, name and value
    /// in a stable order. Equal materials give equal values and a change to any of those changes the value. Texture
    /// and buffer values count by object identity. The value is the engine's choice and does not match any other
    /// engine's hash of a material.
    /// </summary>
    public int ComputeCRC()
    {
        using var stream = new MemoryStream();
        using (var output = new BinaryWriter(stream, Encoding.UTF8, leaveOpen: true))
        {
            output.Write(shader?.name ?? "");
            output.Write(_renderQueue);
            WriteSorted(output, _keywords.Select(keyword => (keyword, "")));
            WriteSorted(output, _overrideTags.Select(tag => (tag.Key, tag.Value)));
            _properties.WriteForCrc(output);
        }
        return unchecked((int)Crc32.Of(stream.ToArray()));
    }

    private static void WriteSorted(BinaryWriter output, IEnumerable<(string Key, string Value)> entries)
    {
        var sorted = entries.OrderBy(entry => entry.Key, StringComparer.Ordinal).ToArray();
        output.Write(sorted.Length);
        foreach (var (key, value) in sorted)
        {
            output.Write(key);
            output.Write(value);
        }
    }
}
