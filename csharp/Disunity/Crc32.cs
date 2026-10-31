namespace Disunity.Hosting;

/// <summary>The CRC-32 of zlib and IEEE 802.3 (reflected polynomial 0xEDB88320, initial value and final xor all ones).</summary>
internal static class Crc32
{
    internal static uint Of(ReadOnlySpan<byte> bytes)
    {
        var crc = 0xFFFFFFFFu;
        foreach (var value in bytes)
        {
            crc ^= value;
            for (var bit = 0; bit < 8; bit++) crc = (crc & 1) != 0 ? (crc >> 1) ^ 0xEDB88320u : crc >> 1;
        }
        return ~crc;
    }
}
