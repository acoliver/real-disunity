namespace UnityEngine;

/// <summary>Offsets from each edge of a rectangle, such as a UI element's padding. Values may be negative.</summary>
[Serializable]
public sealed class RectOffset
{
    public RectOffset()
    {
    }

    public RectOffset(int left, int right, int top, int bottom) => (this.left, this.right, this.top, this.bottom) = (left, right, top, bottom);

    public int left { get; set; }
    public int right { get; set; }
    public int top { get; set; }
    public int bottom { get; set; }

    public int horizontal => left + right;
    public int vertical => top + bottom;

    /// <summary>The rectangle grown outwards by these offsets.</summary>
    public Rect Add(Rect rect) => new(rect.x - left, rect.y - top, rect.width + horizontal, rect.height + vertical);

    /// <summary>The rectangle shrunk inwards by these offsets.</summary>
    public Rect Remove(Rect rect) => new(rect.x + left, rect.y + top, rect.width - horizontal, rect.height - vertical);

    public override string ToString() => $"RectOffset (l:{left} r:{right} t:{top} b:{bottom})";
}
