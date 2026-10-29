namespace UnityEngine;

/// <summary>Conversions between screen points and rectangles. The managed transform's matrix places the
/// rectangle, so a rectangle whose position comes only from native anchor layout is not offset by it.</summary>
public static class RectTransformUtility
{
    /// <summary>The ray through a screen point. With no camera (a screen-space overlay) it starts at (x, y, -100)
    /// and runs along +z; the starting depth is the engine's choice. With a camera it is the camera's own ray.</summary>
    public static Ray ScreenPointToRay(Camera? cam, Vector2 screenPos) =>
        cam == null ? new Ray(new Vector3(screenPos.x, screenPos.y, -100f), Vector3.forward) : cam.ScreenPointToRay(screenPos);

    /// <summary>Intersects the screen ray with the plane of the rectangle (through its position, facing against its forward axis).</summary>
    public static bool ScreenPointToWorldPointInRectangle(RectTransform rect, Vector2 screenPoint, Camera? cam, out Vector3 worldPoint)
    {
        worldPoint = Vector3.zero;
        var ray = ScreenPointToRay(cam, screenPoint);
        var plane = new Plane(rect.rotation * Vector3.back, rect.position);
        var enter = 0f;
        if (!plane.Raycast(ray, ref enter)) { return false; }
        worldPoint = ray.GetPoint(enter);
        return true;
    }

    /// <summary>The hit point in the rectangle's own space; zero when the ray misses the plane.</summary>
    public static bool ScreenPointToLocalPointInRectangle(RectTransform rect, Vector2 screenPoint, Camera? cam, out Vector2 localPoint)
    {
        localPoint = Vector2.zero;
        if (!ScreenPointToWorldPointInRectangle(rect, screenPoint, cam, out var world)) { return false; }
        var local = rect.InverseTransformPoint(world);
        localPoint = new Vector2(local.x, local.y);
        return true;
    }

    public static bool RectangleContainsScreenPoint(RectTransform rect, Vector2 screenPoint, Camera? cam) =>
        RectangleContainsScreenPoint(rect, screenPoint, cam, Vector4.zero);

    /// <summary>The offset (left, bottom, right, top) shrinks the tested rectangle from each side, which is the engine's choice of sign.</summary>
    public static bool RectangleContainsScreenPoint(RectTransform rect, Vector2 screenPoint, Camera? cam, Vector4 offset)
    {
        if (!ScreenPointToLocalPointInRectangle(rect, screenPoint, cam, out var local)) { return false; }
        var r = rect.rect;
        var inner = Rect.MinMaxRect(r.xMin + offset.x, r.yMin + offset.y, r.xMax - offset.z, r.yMax - offset.w);
        return inner.Contains(local);
    }

    /// <summary>The screen point of a world point: the point itself with no camera, else the camera's projection.</summary>
    public static Vector2 WorldToScreenPoint(Camera? cam, Vector3 worldPoint)
    {
        if (cam == null) { return new Vector2(worldPoint.x, worldPoint.y); }
        var screen = cam.WorldToScreenPoint(worldPoint);
        return new Vector2(screen.x, screen.y);
    }

    /// <summary>Returns the point unchanged: the engine does not snap UI to whole pixels here.</summary>
    public static Vector2 PixelAdjustPoint(Vector2 point, Transform elementTransform, Canvas canvas) => point;

    /// <summary>Returns the rectangle unchanged, for the same reason as <see cref="PixelAdjustPoint"/>.</summary>
    public static Rect PixelAdjustRect(RectTransform rectTransform, Canvas canvas) => rectTransform.rect;

    public static void FlipLayoutAxes(RectTransform rect, bool keepPositioning, bool recursive)
    {
        FlipLayoutOnAxis(rect, 0, keepPositioning, recursive);
        FlipLayoutOnAxis(rect, 1, keepPositioning, recursive);
    }

    /// <summary>Mirrors the pivot on an axis (0 for x, 1 for y). Unless <paramref name="keepPositioning"/>, it also
    /// mirrors the anchors and the anchored position. Children are always mirrored with their positioning.</summary>
    public static void FlipLayoutOnAxis(RectTransform rect, int axis, bool keepPositioning, bool recursive)
    {
        if (axis is not (0 or 1)) { throw new ArgumentOutOfRangeException(nameof(axis), "axis must be 0 or 1"); }
        if (recursive)
        {
            for (var i = 0; i < rect.childCount; i++)
            {
                if (rect.GetChild(i) is RectTransform child) { FlipLayoutOnAxis(child, axis, false, true); }
            }
        }
        var pivot = rect.pivot;
        if (axis == 0) { pivot.x = 1f - pivot.x; } else { pivot.y = 1f - pivot.y; }
        rect.pivot = pivot;
        if (keepPositioning) { return; }
        FlipPlacement(rect, axis);
    }

    private static void FlipPlacement(RectTransform rect, int axis)
    {
        var (min, max, position) = (rect.anchorMin, rect.anchorMax, rect.anchoredPosition);
        if (axis == 0) { (min.x, max.x, position.x) = (1f - rect.anchorMax.x, 1f - rect.anchorMin.x, -position.x); }
        else { (min.y, max.y, position.y) = (1f - rect.anchorMax.y, 1f - rect.anchorMin.y, -position.y); }
        rect.SetLayout(min, max, rect.pivot, rect.sizeDelta, position);
    }
}
