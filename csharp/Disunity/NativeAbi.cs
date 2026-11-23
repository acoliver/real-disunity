using Disunity.Hosting;

namespace Disunity.Native;

/// <summary>The native ABI revision this assembly was built against: the 32-bit FNV-1a hash of the ABI
/// surface the native build generated into <c>crates/disunity-ffi/abi/surface.g.txt</c>, which this assembly
/// embeds. The native library hashes the same text, so no number is stored anywhere.</summary>
internal static class NativeAbi
{
    internal static readonly uint Version = Revision(Surface());

    internal static uint Revision(ReadOnlySpan<byte> surface)
    {
        var hash = 0x811c9dc5u;
        foreach (var value in surface)
        {
            hash ^= value;
            hash = unchecked(hash * 0x01000193u);
        }
        return hash;
    }

    internal static void VerifyRevision(uint actual)
    {
        if (actual != Version) throw new DisunityAbiRevisionException(Version, actual);
    }

    private static byte[] Surface()
    {
        using var stream = typeof(NativeAbi).Assembly.GetManifestResourceStream("Disunity.AbiSurface")
            ?? throw new InvalidOperationException("the ABI surface is not embedded in this assembly");
        using var memory = new MemoryStream();
        stream.CopyTo(memory);
        return memory.ToArray();
    }
}
