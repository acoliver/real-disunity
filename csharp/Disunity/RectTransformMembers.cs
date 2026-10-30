using Disunity.Native;

namespace UnityEngine;

/// <summary>Offsets, corners and sizing helpers computed from the native layout record. Corners use
/// the managed transform's matrix, which does not include the position the native UI layout
/// derives from the anchors.</summary>
public sealed partial class RectTransform
{
    /// <summary>Called for a rectangle whose driven properties need applying again. Only an editor or an
    /// animation that drives properties raises it; the engine has neither, so nothing raises it.</summary>
    public delegate void ReapplyDrivenProperties(RectTransform driven);

    public static event ReapplyDrivenProperties? reapplyDrivenProperties;

    /// <summary>Calls the handlers once; tests use it to check registration.</summary>
    internal static void RaiseReapplyDrivenProperties(RectTransform driven) => reapplyDrivenProperties?.Invoke(driven);

    public enum Edge { Left, Right, Top, Bottom }

    public enum Axis { Horizontal, Vertical }

    private bool _sendChildDimensionsChange;

    /// <summary>Stored per rectangle and read by nothing: the engine sends no child dimension
    /// change messages.</summary>
    public bool sendChildDimensionsChange { get => _sendChildDimensionsChange; set => _sendChildDimensionsChange = value; }

    /// <summary>The lower left corner relative to the lower left anchor.</summary>
    public Vector2 offsetMin
    {
        get { var r = Read(); return Min(r); }
        set
        {
            var r = Read();
            var diff = value - Min(r);
            (r.size_x, r.size_y) = (r.size_x - diff.x, r.size_y - diff.y);
            (r.position_x, r.position_y) = (r.position_x + diff.x * (1f - r.pivot_x), r.position_y + diff.y * (1f - r.pivot_y));
            Write(r);
        }
    }

    /// <summary>The upper right corner relative to the upper right anchor.</summary>
    public Vector2 offsetMax
    {
        get { var r = Read(); return Max(r); }
        set
        {
            var r = Read();
            var diff = value - Max(r);
            (r.size_x, r.size_y) = (r.size_x + diff.x, r.size_y + diff.y);
            (r.position_x, r.position_y) = (r.position_x + diff.x * r.pivot_x, r.position_y + diff.y * r.pivot_y);
            Write(r);
        }
    }

    private static Vector2 Min(DisunityRect r) => new(r.position_x - r.size_x * r.pivot_x, r.position_y - r.size_y * r.pivot_y);

    private static Vector2 Max(DisunityRect r) => new(r.position_x + r.size_x * (1f - r.pivot_x), r.position_y + r.size_y * (1f - r.pivot_y));

    /// <summary>The position with the local z as its third component; setting it writes both.</summary>
    public Vector3 anchoredPosition3D
    {
        get { var p = anchoredPosition; return new Vector3(p.x, p.y, localPosition.z); }
        set
        {
            anchoredPosition = new Vector2(value.x, value.y);
            var local = localPosition;
            localPosition = new Vector3(local.x, local.y, value.z);
        }
    }

    /// <summary>Fills four corners of <see cref="rect"/> in the transform's own space: bottom left,
    /// top left, top right, bottom right.</summary>
    /// <exception cref="ArgumentException">The array is null or shorter than four.</exception>
    public void GetLocalCorners(Vector3[] fourCornersArray)
    {
        RequireFour(fourCornersArray);
        var r = rect;
        fourCornersArray[0] = new Vector3(r.xMin, r.yMin, 0f);
        fourCornersArray[1] = new Vector3(r.xMin, r.yMax, 0f);
        fourCornersArray[2] = new Vector3(r.xMax, r.yMax, 0f);
        fourCornersArray[3] = new Vector3(r.xMax, r.yMin, 0f);
    }

    /// <summary>The same corners in world space, through the transform's local to world matrix.</summary>
    /// <exception cref="ArgumentException">The array is null or shorter than four.</exception>
    public void GetWorldCorners(Vector3[] fourCornersArray)
    {
        GetLocalCorners(fourCornersArray);
        var matrix = localToWorldMatrix;
        for (var i = 0; i < 4; i++) { fourCornersArray[i] = matrix.MultiplyPoint3x4(fourCornersArray[i]); }
    }

    private static void RequireFour(Vector3[] corners)
    {
        if (corners == null || corners.Length < 4) { throw new ArgumentException("Calling GetCorners with an array that is null or shorter than 4 elements", nameof(corners)); }
    }

    /// <summary>Sizes the rectangle along an axis by adjusting <see cref="sizeDelta"/> for the
    /// parent's current size and this rectangle's anchors.</summary>
    public void SetSizeWithCurrentAnchors(Axis axis, float size)
    {
        var r = Read();
        var anchorSpan = axis == Axis.Horizontal ? r.max_x - r.min_x : r.max_y - r.min_y;
        var parent = ParentSize();
        if (axis == Axis.Horizontal) { r.size_x = size - parent.x * anchorSpan; } else { r.size_y = size - parent.y * anchorSpan; }
        Write(r);
    }

    private Vector2 ParentSize() => transform.parent is RectTransform parent ? parent.rect.size : Vector2.zero;

    /// <summary>Anchors the rectangle to a parent edge, leaves <paramref name="inset"/> from that
    /// edge and gives it <paramref name="size"/> along that axis. The other axis is unchanged.</summary>
    public void SetInsetAndSizeFromParentEdge(Edge edge, float inset, float size)
    {
        var r = Read();
        var end = edge == Edge.Right || edge == Edge.Top;
        var anchor = end ? 1f : 0f;
        if (edge == Edge.Left || edge == Edge.Right)
        {
            (r.min_x, r.max_x, r.size_x) = (anchor, anchor, size);
            r.position_x = end ? -inset - size * (1f - r.pivot_x) : inset + size * r.pivot_x;
        }
        else
        {
            (r.min_y, r.max_y, r.size_y) = (anchor, anchor, size);
            r.position_y = end ? -inset - size * (1f - r.pivot_y) : inset + size * r.pivot_y;
        }
        Write(r);
    }
}
