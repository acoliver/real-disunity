using Disunity.Hosting;
using UnityEngine;

/// <summary>RectTransform offsets, corners, sizing helpers and RectTransformUtility, against hand-computed values.</summary>
internal static class RectTransformMemberTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Offsets();
            Console.WriteLine("PASS: RectTransform offsetMin and offsetMax read and keep the opposite corner when set");
            Position3D();
            Console.WriteLine("PASS: RectTransform.anchoredPosition3D carries the local z");
            Corners();
            Console.WriteLine("PASS: GetLocalCorners and GetWorldCorners run bottom-left, top-left, top-right, bottom-right");
            Sizing();
            Console.WriteLine("PASS: SetSizeWithCurrentAnchors and SetInsetAndSizeFromParentEdge place the rectangle");
            Flags();
            Console.WriteLine("PASS: RectTransform.sendChildDimensionsChange is stored per instance");
            Overlay();
            Console.WriteLine("PASS: RectTransformUtility with no camera casts from z -100 along +z");
            Perspective();
            Console.WriteLine("PASS: RectTransformUtility through a perspective camera hits, misses and converts points");
            Flip();
            Console.WriteLine("PASS: RectTransformUtility.FlipLayoutAxes mirrors pivot and anchors");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static RectTransform NewCanvas()
    {
        var root = DisunityHost.CreateGameObject();
        var rect = root.AddComponent<RectTransform>();
        root.AddComponent<Canvas>();
        return rect;
    }

    private static RectTransform NewChild(RectTransform parent, Vector2 size, Vector2? pivot = null)
    {
        var child = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        child.SetParent(parent, false);
        child.sizeDelta = size;
        if (pivot is { } p) child.pivot = p;
        return child;
    }

    private static void Same(Vector2 expected, Vector2 actual, string what) =>
        Expect.That((expected - actual).magnitude < 1e-3f, $"{what}: expected {expected}, got {actual}");

    private static void Same(Vector3 expected, Vector3 actual, string what) =>
        Expect.That((expected - actual).magnitude < 1e-3f, $"{what}: expected {expected}, got {actual}");

    private static void Offsets()
    {
        var child = NewChild(NewCanvas(), new Vector2(200, 50));
        child.anchoredPosition = new Vector2(10, 20);
        Same(new Vector2(-90, -5), child.offsetMin, "offsetMin");
        Same(new Vector2(110, 45), child.offsetMax, "offsetMax");
        child.offsetMin = new Vector2(-100, -10);
        Same(new Vector2(110, 45), child.offsetMax, "offsetMax kept");
        Same(new Vector2(210, 55), child.sizeDelta, "size after offsetMin");
        Same(new Vector2(5, 17.5f), child.anchoredPosition, "position after offsetMin");
        child.pivot = new Vector2(0.25f, 0.75f);
        var min = child.offsetMin;
        child.offsetMax = new Vector2(150, 60);
        Same(min, child.offsetMin, "offsetMin kept with an off-center pivot");
        Same(new Vector2(150, 60), child.offsetMax, "offsetMax set");
        Same(child.offsetMax - child.offsetMin, child.sizeDelta, "size is the offset difference");
    }

    private static void Position3D()
    {
        var child = NewChild(NewCanvas(), new Vector2(10, 10));
        child.localPosition = new Vector3(0, 0, 7);
        child.anchoredPosition = new Vector2(3, 4);
        Same(new Vector3(3, 4, 7), child.anchoredPosition3D, "get");
        child.anchoredPosition3D = new Vector3(1, 2, 5);
        Same(new Vector2(1, 2), child.anchoredPosition, "set xy");
        Expect.Near(5, child.localPosition.z, "set z");
    }

    private static void Corners()
    {
        var canvas = NewCanvas();
        var child = NewChild(canvas, new Vector2(200, 50));
        var local = new Vector3[4];
        child.GetLocalCorners(local);
        Same(new Vector3(-100, -25, 0), local[0], "local bottom-left");
        Same(new Vector3(-100, 25, 0), local[1], "local top-left");
        Same(new Vector3(100, 25, 0), local[2], "local top-right");
        Same(new Vector3(100, -25, 0), local[3], "local bottom-right");
        var holder = NewChild(canvas, new Vector2(10, 10));
        holder.localPosition = new Vector3(10, 20, 0);
        holder.localRotation = Quaternion.Euler(0, 0, 90);
        holder.localScale = new Vector3(2, 2, 1);
        var leaf = NewChild(holder, new Vector2(200, 50));
        var world = new Vector3[4];
        leaf.GetWorldCorners(world);
        Same(new Vector3(60, -180, 0), world[0], "world bottom-left");
        Same(new Vector3(-40, -180, 0), world[1], "world top-left");
        Same(new Vector3(-40, 220, 0), world[2], "world top-right");
        Same(new Vector3(60, 220, 0), world[3], "world bottom-right");
        Expect.Throws<ArgumentException>(() => leaf.GetWorldCorners(new Vector3[3]), "short array");
        Expect.Throws<ArgumentException>(() => leaf.GetLocalCorners(null!), "null array");
    }

    private static void Sizing()
    {
        var canvas = NewCanvas();
        var stretched = NewChild(canvas, Vector2.zero);
        stretched.anchorMin = new Vector2(0, 0.5f);
        stretched.anchorMax = new Vector2(1, 0.5f);
        stretched.SetSizeWithCurrentAnchors(RectTransform.Axis.Horizontal, 300);
        Same(new Vector2(-500, 0), stretched.sizeDelta, "stretched size delta");
        Expect.Near(300, stretched.rect.width, "stretched width");
        stretched.SetSizeWithCurrentAnchors(RectTransform.Axis.Vertical, 40);
        Same(new Vector2(-500, 40), stretched.sizeDelta, "fixed height delta");
        var inset = NewChild(canvas, new Vector2(5, 5), new Vector2(0.25f, 0.75f));
        inset.SetInsetAndSizeFromParentEdge(RectTransform.Edge.Left, 10, 100);
        Same(new Vector2(0, 0.5f), inset.anchorMin, "left anchor min");
        Same(new Vector2(0, 0.5f), inset.anchorMax, "left anchor max");
        Expect.Near(100, inset.sizeDelta.x, "left size");
        Expect.Near(35, inset.anchoredPosition.x, "left position");
        inset.SetInsetAndSizeFromParentEdge(RectTransform.Edge.Right, 10, 100);
        Expect.Near(1, inset.anchorMin.x, "right anchor");
        Expect.Near(-85, inset.anchoredPosition.x, "right position");
        inset.SetInsetAndSizeFromParentEdge(RectTransform.Edge.Top, 10, 80);
        Expect.Near(1, inset.anchorMax.y, "top anchor");
        Expect.Near(80, inset.sizeDelta.y, "top size");
        Expect.Near(-30, inset.anchoredPosition.y, "top position");
        inset.SetInsetAndSizeFromParentEdge(RectTransform.Edge.Bottom, 10, 80);
        Expect.Near(0, inset.anchorMin.y, "bottom anchor");
        Expect.Near(70, inset.anchoredPosition.y, "bottom position");
        Expect.Near(-85, inset.anchoredPosition.x, "other axis untouched");
    }

    private static void Flags()
    {
        var canvas = NewCanvas();
        var a = NewChild(canvas, Vector2.one);
        var b = NewChild(canvas, Vector2.one);
        Expect.That(!a.sendChildDimensionsChange && !b.sendChildDimensionsChange, "default off");
        a.sendChildDimensionsChange = true;
        Expect.That(a.sendChildDimensionsChange && !b.sendChildDimensionsChange, "stored per instance");
    }

    private static void Overlay()
    {
        var canvas = NewCanvas();
        var child = NewChild(canvas, new Vector2(200, 50));
        child.localPosition = new Vector3(300, 200, 0);
        Expect.That(RectTransformUtility.ScreenPointToLocalPointInRectangle(child, new Vector2(310, 210), null, out var local), "overlay hit");
        Same(new Vector2(10, 10), local, "overlay local point");
        Expect.That(RectTransformUtility.ScreenPointToWorldPointInRectangle(child, new Vector2(310, 210), null, out var world), "overlay world hit");
        Same(new Vector3(310, 210, 0), world, "overlay world point");
        Expect.That(RectTransformUtility.RectangleContainsScreenPoint(child, new Vector2(390, 220), null), "inside");
        Expect.That(!RectTransformUtility.RectangleContainsScreenPoint(child, new Vector2(410, 200), null), "outside");
        Expect.That(!RectTransformUtility.RectangleContainsScreenPoint(child, new Vector2(390, 220), null, new Vector4(0, 0, 20, 0)), "offset shrinks the right edge");
        child.localRotation = Quaternion.Euler(0, 90, 0);
        Expect.That(!RectTransformUtility.ScreenPointToLocalPointInRectangle(child, new Vector2(300, 200), null, out var missed), "edge-on plane misses");
        Same(Vector2.zero, missed, "miss local point");
        var ray = RectTransformUtility.ScreenPointToRay(null, new Vector2(5, 6));
        Same(new Vector3(5, 6, -100), ray.origin, "overlay ray origin");
        Same(Vector3.forward, ray.direction, "overlay ray direction");
        Same(new Vector2(5, 6), RectTransformUtility.WorldToScreenPoint(null, new Vector3(5, 6, 9)), "overlay world to screen");
        Same(new Vector2(3, 4), RectTransformUtility.PixelAdjustPoint(new Vector2(3, 4), child, null!), "pixel adjust point");
    }

    private static void Perspective()
    {
        var canvasObject = DisunityHost.CreateGameObject();
        var plane = canvasObject.AddComponent<RectTransform>();
        canvasObject.AddComponent<Canvas>().renderMode = RenderMode.WorldSpace;
        plane.sizeDelta = new Vector2(100, 100);
        var owner = DisunityHost.CreateGameObject();
        owner.transform.position = new Vector3(0, 0, -10);
        var camera = owner.AddComponent<Camera>();
        camera.orthographic = false;
        camera.fieldOfView = 90;
        Expect.That(RectTransformUtility.ScreenPointToLocalPointInRectangle(plane, new Vector2(400, 300), camera, out var center), "center hit");
        Same(Vector2.zero, center, "center local point");
        Expect.That(RectTransformUtility.ScreenPointToLocalPointInRectangle(plane, new Vector2(400, 600), camera, out var top), "top hit");
        Same(new Vector2(0, 10), top, "top local point");
        Expect.That(RectTransformUtility.RectangleContainsScreenPoint(plane, new Vector2(400, 300), camera), "contains center");
        plane.localPosition = new Vector3(0, 0, -20);
        Expect.That(!RectTransformUtility.ScreenPointToLocalPointInRectangle(plane, new Vector2(400, 300), camera, out _), "plane behind the camera misses");
        plane.localPosition = Vector3.zero;
        var screen = RectTransformUtility.WorldToScreenPoint(camera, new Vector3(0, 10, 0));
        Same(new Vector2(400, 600), screen, "world to screen");
        var ray = RectTransformUtility.ScreenPointToRay(camera, new Vector2(400, 300));
        Same(new Vector3(0, 0, -10), ray.origin, "camera ray origin");
    }

    private static void Flip()
    {
        var canvas = NewCanvas();
        var parent = NewChild(canvas, new Vector2(10, 10), new Vector2(0.25f, 0.5f));
        parent.anchorMin = new Vector2(0, 0);
        parent.anchorMax = new Vector2(0.25f, 1);
        parent.anchoredPosition = new Vector2(7, 3);
        var child = NewChild(parent, new Vector2(4, 4), new Vector2(0.1f, 0.5f));
        RectTransformUtility.FlipLayoutAxes(parent, false, true);
        Same(new Vector2(0.75f, 0.5f), parent.pivot, "pivot flipped on x");
        Same(new Vector2(0.75f, 0), parent.anchorMin, "anchor min flipped");
        Same(new Vector2(1, 1), parent.anchorMax, "anchor max flipped");
        Same(new Vector2(-7, -3), parent.anchoredPosition, "position flipped");
        Same(new Vector2(0.9f, 0.5f), child.pivot, "child pivot flipped recursively");
        RectTransformUtility.FlipLayoutOnAxis(parent, 0, true, false);
        Same(new Vector2(0.25f, 0.5f), parent.pivot, "keep positioning pivot");
        Same(new Vector2(-7, -3), parent.anchoredPosition, "keep positioning leaves the offset");
    }
}
