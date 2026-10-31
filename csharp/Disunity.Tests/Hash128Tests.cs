using Unity.Collections;
using UnityEngine;

/// <summary>Hash128 text form, hashing, equality and ordering; BoundingSphere data.</summary>
internal static class Hash128Tests
{
    public static void Run()
    {
        TextForm();
        Computing();
        Appending();
        Ordering();
        Spheres();
        Console.WriteLine("PASS: Hash128 parse, format, compute, append and ordering; BoundingSphere data");
    }

    private static void TextForm()
    {
        var hash = new Hash128(0x0123456789abcdefUL, 0xfedcba9876543210UL);
        Expect.Equal("0123456789abcdeffedcba9876543210", hash.ToString(), "32 hex digits, first word first");
        Expect.Equal(hash, Hash128.Parse(hash.ToString()), "parse round trip");
        Expect.Equal(hash, Hash128.Parse("0123456789ABCDEFFEDCBA9876543210"), "parse accepts upper case");
        Expect.Equal(new string('0', 32), default(Hash128).ToString(), "a default hash prints 32 zeros");
        Expect.That(!default(Hash128).isValid, "a default hash is invalid");
        Expect.That(hash.isValid, "a nonzero hash is valid");
        Expect.That(!Hash128.Parse("abc").isValid, "a short string parses to an invalid hash");
        Expect.That(!Hash128.Parse(new string('g', 32)).isValid, "a non-hex string parses to an invalid hash");
        Expect.That(!Hash128.Parse(new string('a', 33)).isValid, "a long string parses to an invalid hash");
        Expect.Throws<ArgumentNullException>(() => Hash128.Parse(null!), "parse null");
        var words = new Hash128(1u, 2u, 3u, 4u);
        Expect.Equal("00000001000000020000000300000004", words.ToString(), "four words fill the digits in order");
    }

    private static void Computing()
    {
        Expect.Equal(Hash128.Compute("a"), Hash128.Compute("a"), "string hash is deterministic");
        Expect.That(Hash128.Compute("a") != Hash128.Compute("b"), "different strings differ");
        Expect.That(Hash128.Compute("a").isValid, "a computed hash is valid");
        Expect.Equal(Hash128.Compute(5), Hash128.Compute(5), "int hash is deterministic");
        Expect.That(Hash128.Compute(5) != Hash128.Compute(6), "different ints differ");
        Expect.Equal(Hash128.Compute(1.5f), Hash128.Compute(1.5f), "float hash is deterministic");
        Expect.That(Hash128.Compute(1.5f) != Hash128.Compute(2.5f), "different floats differ");
        Expect.Equal(Hash128.Compute(new[] { 1, 2, 3 }), Hash128.Compute(new List<int> { 1, 2, 3 }), "array and list of the same ints agree");
        Expect.That(Hash128.Compute(new[] { 1, 2, 3 }) != Hash128.Compute(new[] { 3, 2, 1 }), "order matters");
        using var native = new NativeArray<int>(new[] { 1, 2, 3 }, Allocator.Temp);
        Expect.Equal(Hash128.Compute(new[] { 1, 2, 3 }), Hash128.Compute(native), "native array agrees with the managed array");
        Expect.Throws<ArgumentNullException>(() => Hash128.Compute((string)null!), "compute null string");
    }

    private static void Appending()
    {
        var hash = new Hash128();
        hash.Append("a");
        var once = hash;
        hash.Append("a");
        Expect.That(hash != once, "appending changes the hash");
        var other = new Hash128();
        other.Append("a");
        other.Append("a");
        Expect.Equal(hash, other, "the same appends give the same hash");
        var reversed = new Hash128();
        reversed.Append(1);
        reversed.Append(2);
        var forward = new Hash128();
        forward.Append(2);
        forward.Append(1);
        Expect.That(reversed != forward, "append order matters");
        var seedA = new Hash128(1UL, 2UL);
        var seedB = new Hash128(1UL, 3UL);
        seedA.Append("x");
        seedB.Append("x");
        Expect.That(seedA != seedB, "both words of the current hash feed the next one");
        var floats = new Hash128();
        floats.Append(1f);
        Expect.That(floats.isValid, "appending a float makes the hash valid");
        var span = new Hash128();
        span.Append(new[] { 1, 2 });
        Expect.That(span != default, "appending an array changes the hash");
    }

    private static void Ordering()
    {
        var low = new Hash128(1UL, ulong.MaxValue);
        var high = new Hash128(2UL, 0UL);
        Expect.That(low < high && high > low, "the first word orders first");
        var same = low;
        Expect.That(low <= same && low >= same, "a hash is equal to itself in order");
        Expect.That(new Hash128(1UL, 1UL) < new Hash128(1UL, 2UL), "the second word breaks a tie");
        Expect.That(new Hash128(0UL, ulong.MaxValue) < new Hash128(1UL, 0UL), "words compare as unsigned");
        Expect.Equal(-1, low.CompareTo(high), "CompareTo less");
        Expect.Equal(1, high.CompareTo(low), "CompareTo greater");
        Expect.Equal(0, low.CompareTo(low), "CompareTo equal");
        Expect.That(low.Equals((object)new Hash128(1UL, ulong.MaxValue)), "object equality");
        Expect.That(!low.Equals("x"), "another type is unequal");
        Expect.Equal(low.GetHashCode(), new Hash128(1UL, ulong.MaxValue).GetHashCode(), "equal hashes have equal hash codes");
        Expect.That(low.u64_0 == 1UL && low.u64_1 == ulong.MaxValue, "words read back");
    }

    private static void Spheres()
    {
        var sphere = new BoundingSphere(new Vector3(1, 2, 3), 4f);
        Expect.Equal(new Vector3(1, 2, 3), sphere.position, "position");
        Expect.Near(4f, sphere.radius, "radius");
        var fromVector = new BoundingSphere(new Vector4(1, 2, 3, 4));
        Expect.Equal(sphere, fromVector, "a Vector4 holds the position and then the radius");
        Expect.That(sphere == fromVector && !(sphere != fromVector), "== and !=");
        Expect.That(sphere != new BoundingSphere(new Vector3(1, 2, 3), 5f), "different radius is unequal");
        Expect.That(sphere != new BoundingSphere(new Vector3(0, 2, 3), 4f), "different position is unequal");
        Expect.Equal(sphere.GetHashCode(), fromVector.GetHashCode(), "equal spheres have equal hash codes");
        Expect.That(sphere.Equals((object)fromVector) && !sphere.Equals(null), "object equality");
        Expect.Equal(0f, default(BoundingSphere).radius, "a default sphere has no radius");
    }
}
