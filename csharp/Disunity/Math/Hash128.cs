using System.Buffers.Binary;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Unity.Collections;

namespace UnityEngine;

/// <summary>
/// A 128-bit hash made of two 64-bit words. The text form is 32 lowercase hexadecimal digits, the first word
/// then the second. Hashing is the engine's choice: <c>Append</c> replaces the hash with the first 16 bytes
/// of SHA-256 over the current 16 bytes (each word little-endian) followed by the appended bytes, so the
/// result depends on the order of appends and is the same on every platform. Strings append as UTF-8, ints and
/// floats as four little-endian bytes, and arrays and lists as the bytes of their elements in memory order.
/// A default hash is invalid; a computed hash is valid unless the digest happens to be zero.
/// </summary>
public struct Hash128 : IEquatable<Hash128>, IComparable<Hash128>
{
    private ulong m_u64_0;
    private ulong m_u64_1;

    public Hash128(uint u32_0, uint u32_1, uint u32_2, uint u32_3) :
        this(((ulong)u32_0 << 32) | u32_1, ((ulong)u32_2 << 32) | u32_3) { }

    public Hash128(ulong u64_0, ulong u64_1)
    {
        m_u64_0 = u64_0;
        m_u64_1 = u64_1;
    }

    public readonly ulong u64_0 => m_u64_0;
    public readonly ulong u64_1 => m_u64_1;
    public readonly bool isValid => (m_u64_0 | m_u64_1) != 0;

    /// <summary>The hash for 32 hexadecimal digits of either case, or an invalid hash for any other text.</summary>
    public static Hash128 Parse(string hashString)
    {
        ArgumentNullException.ThrowIfNull(hashString);
        if (hashString.Length != 32 || !hashString.All(char.IsAsciiHexDigit)) return default;
        return new Hash128(
            ulong.Parse(hashString.AsSpan(0, 16), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture),
            ulong.Parse(hashString.AsSpan(16, 16), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture));
    }

    public static Hash128 Compute(string data)
    {
        var hash = default(Hash128);
        hash.Append(data);
        return hash;
    }

    public static Hash128 Compute(int data)
    {
        var hash = default(Hash128);
        hash.Append(data);
        return hash;
    }

    public static Hash128 Compute(float data)
    {
        var hash = default(Hash128);
        hash.Append(data);
        return hash;
    }

    public static Hash128 Compute<T>(T[] data) where T : struct
    {
        var hash = default(Hash128);
        hash.Append(data);
        return hash;
    }

    public static Hash128 Compute<T>(List<T> data) where T : struct
    {
        var hash = default(Hash128);
        hash.Append(data);
        return hash;
    }

    public static Hash128 Compute<T>(NativeArray<T> data) where T : struct
    {
        var hash = default(Hash128);
        hash.Append(data);
        return hash;
    }

    public void Append(string data)
    {
        ArgumentNullException.ThrowIfNull(data);
        AppendBytes(Encoding.UTF8.GetBytes(data));
    }

    public void Append(int data) => AppendBytes(BitConverter.GetBytes(data));
    public void Append(float data) => Append(BitConverter.SingleToInt32Bits(data));

    public void Append<T>(T[] data) where T : struct
    {
        ArgumentNullException.ThrowIfNull(data);
        AppendBytes(MemoryMarshal.AsBytes(data.AsSpan()));
    }

    public void Append<T>(List<T> data) where T : struct
    {
        ArgumentNullException.ThrowIfNull(data);
        AppendBytes(MemoryMarshal.AsBytes(CollectionsMarshal.AsSpan(data)));
    }

    public void Append<T>(NativeArray<T> data) where T : struct
    {
        if (!data.IsCreated) throw new ArgumentException("the native array is not created", nameof(data));
        AppendBytes(MemoryMarshal.AsBytes(data.AsReadOnlySpan()));
    }

    private void AppendBytes(ReadOnlySpan<byte> data)
    {
        var input = new byte[16 + data.Length];
        BinaryPrimitives.WriteUInt64LittleEndian(input, m_u64_0);
        BinaryPrimitives.WriteUInt64LittleEndian(input.AsSpan(8), m_u64_1);
        data.CopyTo(input.AsSpan(16));
        Span<byte> digest = stackalloc byte[32];
        SHA256.HashData(input, digest);
        m_u64_0 = BinaryPrimitives.ReadUInt64LittleEndian(digest);
        m_u64_1 = BinaryPrimitives.ReadUInt64LittleEndian(digest[8..]);
    }

    public readonly int CompareTo(Hash128 rhs) => m_u64_0 != rhs.m_u64_0 ? m_u64_0.CompareTo(rhs.m_u64_0) : m_u64_1.CompareTo(rhs.m_u64_1);
    public readonly bool Equals(Hash128 other) => m_u64_0 == other.m_u64_0 && m_u64_1 == other.m_u64_1;
    public override readonly bool Equals(object? obj) => obj is Hash128 other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(m_u64_0, m_u64_1);
    public override readonly string ToString() => $"{m_u64_0:x16}{m_u64_1:x16}";

    public static bool operator ==(Hash128 hash1, Hash128 hash2) => hash1.Equals(hash2);
    public static bool operator !=(Hash128 hash1, Hash128 hash2) => !hash1.Equals(hash2);
    public static bool operator <(Hash128 x, Hash128 y) => x.CompareTo(y) < 0;
    public static bool operator >(Hash128 x, Hash128 y) => x.CompareTo(y) > 0;
    public static bool operator <=(Hash128 x, Hash128 y) => x.CompareTo(y) <= 0;
    public static bool operator >=(Hash128 x, Hash128 y) => x.CompareTo(y) >= 0;
}
