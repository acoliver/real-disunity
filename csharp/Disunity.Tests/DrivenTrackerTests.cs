using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>DrivenRectTransformTracker keeps nothing in an engine without an editor, and
/// DrivenTransformProperties carries the standard flag values.</summary>
internal static class DrivenTrackerTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Tracker();
            Console.WriteLine("PASS: DrivenRectTransformTracker Add and Clear do nothing and Add rejects a null rectangle");
        }
        finally { DisunityHost.UnloadWorld(); }
        Flags();
        Console.WriteLine("PASS: DrivenTransformProperties has the standard single and combined flag values");
    }

    private static void Tracker()
    {
        var owner = DisunityHost.CreateGameObject();
        var rect = owner.AddComponent<RectTransform>();
        rect.sizeDelta = new Vector2(30, 40);
        var tracker = new DrivenRectTransformTracker();
        tracker.Add(owner, rect, DrivenTransformProperties.AnchoredPosition | DrivenTransformProperties.SizeDelta);
        tracker.Add(null!, rect, DrivenTransformProperties.All);
        tracker.Clear();
        tracker.Clear();
        DrivenRectTransformTracker.StartRecordingUndo();
        DrivenRectTransformTracker.StopRecordingUndo();
        var error = Expect.Catch<ArgumentNullException>(() => tracker.Add(owner, null!, DrivenTransformProperties.None), "null rectangle");
        Ensure(error.ParamName == "rectTransform", "the exception names the parameter");
        Ensure(rect.sizeDelta == new Vector2(30, 40), "driving leaves the rectangle alone");
    }

    private static void Flags()
    {
        Ensure((int)DrivenTransformProperties.None == 0 && (int)DrivenTransformProperties.All == -1, "None and All");
        Ensure((int)DrivenTransformProperties.AnchoredPositionX == 2 && (int)DrivenTransformProperties.AnchoredPositionY == 4 && (int)DrivenTransformProperties.AnchoredPositionZ == 8, "position axes");
        Ensure((int)DrivenTransformProperties.Rotation == 16 && (int)DrivenTransformProperties.ScaleX == 32 && (int)DrivenTransformProperties.ScaleY == 64 && (int)DrivenTransformProperties.ScaleZ == 128, "rotation and scale");
        Ensure((int)DrivenTransformProperties.AnchorMinX == 256 && (int)DrivenTransformProperties.AnchorMinY == 512 && (int)DrivenTransformProperties.AnchorMaxX == 1024 && (int)DrivenTransformProperties.AnchorMaxY == 2048, "anchors");
        Ensure((int)DrivenTransformProperties.SizeDeltaX == 4096 && (int)DrivenTransformProperties.SizeDeltaY == 8192 && (int)DrivenTransformProperties.PivotX == 16384 && (int)DrivenTransformProperties.PivotY == 32768, "size and pivot");
        Ensure(DrivenTransformProperties.AnchoredPosition == (DrivenTransformProperties.AnchoredPositionX | DrivenTransformProperties.AnchoredPositionY), "AnchoredPosition");
        Ensure(DrivenTransformProperties.AnchoredPosition3D == (DrivenTransformProperties.AnchoredPosition | DrivenTransformProperties.AnchoredPositionZ), "AnchoredPosition3D");
        Ensure(DrivenTransformProperties.Scale == (DrivenTransformProperties.ScaleX | DrivenTransformProperties.ScaleY | DrivenTransformProperties.ScaleZ), "Scale");
        Ensure(DrivenTransformProperties.Anchors == (DrivenTransformProperties.AnchorMin | DrivenTransformProperties.AnchorMax), "Anchors");
        Ensure(DrivenTransformProperties.AnchorMin == (DrivenTransformProperties.AnchorMinX | DrivenTransformProperties.AnchorMinY), "AnchorMin");
        Ensure(DrivenTransformProperties.AnchorMax == (DrivenTransformProperties.AnchorMaxX | DrivenTransformProperties.AnchorMaxY), "AnchorMax");
        Ensure(DrivenTransformProperties.SizeDelta == (DrivenTransformProperties.SizeDeltaX | DrivenTransformProperties.SizeDeltaY), "SizeDelta");
        Ensure(DrivenTransformProperties.Pivot == (DrivenTransformProperties.PivotX | DrivenTransformProperties.PivotY), "Pivot");
    }
}
