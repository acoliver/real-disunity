namespace UnityEngine.Rendering;

/// <summary>
/// A fixed list of attachment indices, at most <see cref="MaxAttachments"/> long, that a sub-pass reads or writes. A new list
/// of a length holds that many indices, each -1 (no attachment); <see cref="Length"/> is the count of indices in use and
/// the default value has none. An index outside 0 to <c>Length - 1</c> throws <see cref="IndexOutOfRangeException"/> and a length
/// outside 0 to <see cref="MaxAttachments"/> throws <see cref="ArgumentOutOfRangeException"/>. Plain data; the engine does not run
/// render passes yet.
/// </summary>
public struct AttachmentIndexArray : IEquatable<AttachmentIndexArray>
{
    public const int MaxAttachments = 8;

    private int _length;
    private int _i0, _i1, _i2, _i3, _i4, _i5, _i6, _i7;

    public AttachmentIndexArray(int length)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(length);
        ArgumentOutOfRangeException.ThrowIfGreaterThan(length, MaxAttachments);
        (_length, _i0, _i1, _i2, _i3, _i4, _i5, _i6, _i7) = (length, -1, -1, -1, -1, -1, -1, -1, -1);
    }

    public readonly int Length => _length;

    public int this[int index]
    {
        readonly get => index < 0 || index >= _length ? throw new IndexOutOfRangeException($"attachment index {index} is outside 0 to {_length - 1}") : Get(index);
        set
        {
            if (index < 0 || index >= _length) throw new IndexOutOfRangeException($"attachment index {index} is outside 0 to {_length - 1}");
            Set(index, value);
        }
    }

    private readonly int Get(int index) => index switch
    {
        0 => _i0, 1 => _i1, 2 => _i2, 3 => _i3, 4 => _i4, 5 => _i5, 6 => _i6, _ => _i7,
    };

    private void Set(int index, int value)
    {
        switch (index)
        {
            case 0: _i0 = value; break;
            case 1: _i1 = value; break;
            case 2: _i2 = value; break;
            case 3: _i3 = value; break;
            case 4: _i4 = value; break;
            case 5: _i5 = value; break;
            case 6: _i6 = value; break;
            default: _i7 = value; break;
        }
    }

    public readonly bool Equals(AttachmentIndexArray other) =>
        _length == other._length && _i0 == other._i0 && _i1 == other._i1 && _i2 == other._i2 && _i3 == other._i3
        && _i4 == other._i4 && _i5 == other._i5 && _i6 == other._i6 && _i7 == other._i7;

    public override readonly bool Equals(object? obj) => obj is AttachmentIndexArray other && Equals(other);

    public override readonly int GetHashCode() => HashCode.Combine(_length, HashCode.Combine(_i0, _i1, _i2, _i3, _i4, _i5, _i6), _i7);

    public static bool operator ==(AttachmentIndexArray left, AttachmentIndexArray right) => left.Equals(right);
    public static bool operator !=(AttachmentIndexArray left, AttachmentIndexArray right) => !left.Equals(right);
}

/// <summary>
/// One sub-pass of a render pass: the attachments it reads as input, the color attachments it writes and its flags. Plain
/// data; a new descriptor has empty lists and no flags, and the engine does not run render passes yet.
/// </summary>
public struct SubPassDescriptor : IEquatable<SubPassDescriptor>
{
    // Fields: the render graph's native pass compiler reads and writes them as fields.
    public AttachmentIndexArray inputs;
    public AttachmentIndexArray colorOutputs;
    public SubPassFlags flags;

    public readonly bool Equals(SubPassDescriptor other) => inputs == other.inputs && colorOutputs == other.colorOutputs && flags == other.flags;
    public override readonly bool Equals(object? obj) => obj is SubPassDescriptor other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(inputs, colorOutputs, flags);
    public static bool operator ==(SubPassDescriptor left, SubPassDescriptor right) => left.Equals(right);
    public static bool operator !=(SubPassDescriptor left, SubPassDescriptor right) => !left.Equals(right);
}
