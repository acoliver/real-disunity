namespace UnityEngine;

/// <summary>A range of integers described by its first value and its length.</summary>
public struct RangeInt(int start, int length)
{
    public int start = start;
    public int length = length;

    public readonly int end => start + length;
}
