using System.Buffers.Binary;
using System.Reflection.PortableExecutable;

namespace Disunity.Hosting;

/// <summary>
/// Reads how a player's assembly image is flagged for the process that loads it.
/// </summary>
internal static class PlayerImages
{
    /// <summary>
    /// The image at <paramref name="path"/> with its 32-bit-required flag cleared, when it is an IL-only image with
    /// that flag, or null to load the file as it is. A player's runtime ignores the flag on an IL-only image, which
    /// carries no native code, but a 64-bit process refuses to load such an image, so the engine loads it from this
    /// copy instead; an assembly loaded from the copy has no file location. A mixed-mode image keeps the flag.
    /// </summary>
    public static byte[]? WithoutThirtyTwoBitRequirement(string path)
    {
        int offset;
        CorFlags flags;
        using (var reader = new PEReader(File.OpenRead(path)))
        {
            var headers = reader.PEHeaders;
            if (headers.CorHeader is not { } cor) return null;
            const CorFlags required = CorFlags.ILOnly | CorFlags.Requires32Bit;
            if ((cor.Flags & required) != required) return null;
            offset = headers.CorHeaderStartOffset + 16;
            flags = cor.Flags & ~CorFlags.Requires32Bit;
        }
        var image = File.ReadAllBytes(path);
        BinaryPrimitives.WriteUInt32LittleEndian(image.AsSpan(offset, sizeof(uint)), (uint)flags);
        return image;
    }
}
