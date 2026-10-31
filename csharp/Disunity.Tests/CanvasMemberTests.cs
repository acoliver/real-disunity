using Disunity.Hosting;
using UnityEngine;

/// <summary>Canvas sorting, display and shader settings, root canvases, pixelRect, worldCamera and the
/// events raised before a frame is drawn.</summary>
internal static class CanvasMemberTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Settings();
            Console.WriteLine("PASS: Canvas stores overrideSorting, sortingLayerID, targetDisplay, additionalShaderChannels and referencePixelsPerUnit");
            SortingLayerName();
            Console.WriteLine("PASS: Canvas.sortingLayerName maps the Default layer and keeps the layer for an unregistered name");
            RootCanvas();
            Console.WriteLine("PASS: isRootCanvas and rootCanvas follow the enabled canvases above");
            PixelRect();
            Console.WriteLine("PASS: Canvas.pixelRect is the screen, or the world camera's pixels in world space");
            WorldCamera();
            Console.WriteLine("PASS: Canvas.worldCamera can be set for camera and world space canvases");
        }
        finally { DisunityHost.UnloadWorld(); }
        FrameEvents();
        Console.WriteLine("PASS: willRenderCanvases follow preWillRenderCanvases once per frame, after LateUpdate");
        ForcedUpdate();
        Console.WriteLine("PASS: ForceUpdateCanvases raises both events and a throwing handler does not stop the next");
    }

    private static Canvas NewCanvas(GameObject? parent = null, RenderMode mode = RenderMode.WorldSpace)
    {
        var owner = DisunityHost.CreateGameObject();
        owner.AddComponent<RectTransform>();
        var canvas = owner.AddComponent<Canvas>();
        canvas.renderMode = mode;
        if (parent != null) owner.transform.SetParent(parent.transform, false);
        return canvas;
    }

    private static Camera NewCamera() => DisunityHost.CreateGameObject().AddComponent<Camera>();

    private static void Settings()
    {
        var canvas = NewCanvas();
        Expect.That(!canvas.overrideSorting && canvas.sortingLayerID == 0 && canvas.targetDisplay == 0, "defaults");
        Expect.That(canvas.additionalShaderChannels == AdditionalCanvasShaderChannels.None && canvas.referencePixelsPerUnit == 100f, "channel and pixel defaults");
        canvas.overrideSorting = true;
        canvas.sortingLayerID = 9;
        canvas.targetDisplay = 2;
        canvas.additionalShaderChannels = AdditionalCanvasShaderChannels.TexCoord1 | AdditionalCanvasShaderChannels.Tangent;
        canvas.referencePixelsPerUnit = 32f;
        Expect.That(canvas.overrideSorting && canvas.sortingLayerID == 9 && canvas.targetDisplay == 2, "stored settings");
        Expect.Equal(17, (int)canvas.additionalShaderChannels, "channel flags");
        Expect.Near(32f, canvas.referencePixelsPerUnit, "reference pixels");
        canvas.sortingOrder = 4;
        Expect.That(canvas.overrideSorting && canvas.sortingLayerID == 9, "an unrelated write keeps the settings");
        Expect.Throws<DisunityException>(() => canvas.referencePixelsPerUnit = 0f, "zero reference pixels");
        Expect.Throws<DisunityException>(() => canvas.additionalShaderChannels = (AdditionalCanvasShaderChannels)64, "unknown channel");
        Expect.Near(32f, canvas.referencePixelsPerUnit, "rejected write leaves the value");
    }

    private static void SortingLayerName()
    {
        var canvas = NewCanvas();
        Expect.Equal("Default", canvas.sortingLayerName, "default layer name");
        canvas.sortingLayerID = 3;
        Expect.Equal("", canvas.sortingLayerName, "unregistered id has no name");
        canvas.sortingLayerName = "Default";
        Expect.Equal(0, canvas.sortingLayerID, "named Default");
        canvas.sortingLayerID = 3;
        canvas.sortingLayerName = "UI";
        Expect.Equal(3, canvas.sortingLayerID, "an unregistered name keeps the layer");
    }

    private static void RootCanvas()
    {
        var a = NewCanvas();
        var b = NewCanvas(a.gameObject);
        var plain = DisunityHost.CreateGameObject();
        plain.transform.SetParent(b.transform, false);
        var c = NewCanvas(plain);
        Expect.That(a.isRootCanvas && !b.isRootCanvas && !c.isRootCanvas, "root flags");
        Expect.That(ReferenceEquals(a.rootCanvas, a) && ReferenceEquals(b.rootCanvas, a) && ReferenceEquals(c.rootCanvas, a), "topmost canvas");
        b.enabled = false;
        Expect.That(ReferenceEquals(c.rootCanvas, a) && !c.isRootCanvas, "a disabled middle canvas is skipped");
        b.enabled = true;
        a.enabled = false;
        Expect.That(b.isRootCanvas && ReferenceEquals(c.rootCanvas, b), "a disabled top canvas is skipped");
    }

    private static void PixelRect()
    {
        var overlay = NewCanvas(null, RenderMode.ScreenSpaceOverlay);
        Expect.That(overlay.pixelRect == new Rect(0, 0, Screen.width, Screen.height), "overlay is the screen");
        var world = NewCanvas();
        Expect.That(world.pixelRect == new Rect(0, 0, Screen.width, Screen.height), "world space without a camera is the screen");
        var camera = NewCamera();
        camera.rect = new Rect(0, 0, 0.5f, 1f);
        world.worldCamera = camera;
        Expect.That(world.pixelRect == camera.pixelRect && camera.pixelRect.width == Screen.width / 2f, "world space uses the camera's pixels");
    }

    private static void WorldCamera()
    {
        var first = NewCamera();
        var second = NewCamera();
        var canvas = NewCanvas(null, RenderMode.ScreenSpaceOverlay);
        DisunityHost.RenderThroughCamera(canvas, first, 50f);
        canvas.worldCamera = second;
        Expect.That(ReferenceEquals(canvas.worldCamera, second) && canvas.renderMode == RenderMode.ScreenSpaceCamera, "rebound camera");
        Expect.Near(50f, canvas.planeDistance, "plane distance kept");
        Expect.Throws<NotSupportedException>(() => canvas.worldCamera = null, "no camera for a camera canvas");
        var world = NewCanvas();
        world.worldCamera = first;
        Expect.That(ReferenceEquals(world.worldCamera, first), "event camera of a world space canvas");
        world.worldCamera = null;
        Expect.That(world.worldCamera is null, "cleared event camera");
    }

    private sealed class Probe : MonoBehaviour
    {
        public readonly List<string> Log = new();
        private void LateUpdate() => Log.Add("late");
        private System.Collections.IEnumerator End() { yield return new WaitForEndOfFrame(); Log.Add("end"); }
        private void Start() => StartCoroutine(End());
    }

    private static void FrameEvents()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        Canvas.WillRenderCanvases will = () => probe.Log.Add("will");
        Canvas.WillRenderCanvases pre = () => probe.Log.Add("pre");
        try
        {
            Canvas.willRenderCanvases += will;
            Canvas.preWillRenderCanvases += pre;
            DisunityHost.Step(0.01f);
            Expect.Sequence(new[] { "late", "pre", "will", "end" }, probe.Log, "first frame");
            DisunityHost.Step(0.01f);
            Expect.Sequence(new[] { "late", "pre", "will", "end", "late", "pre", "will" }, probe.Log, "once per frame");
            Canvas.willRenderCanvases -= will;
            Canvas.preWillRenderCanvases -= pre;
            DisunityHost.Step(0.01f);
            Expect.Sequence(new[] { "late", "pre", "will", "end", "late", "pre", "will", "late" }, probe.Log, "removed handlers stop");
        }
        finally
        {
            Canvas.willRenderCanvases -= will;
            Canvas.preWillRenderCanvases -= pre;
            DisunityHost.UnloadWorld();
        }
    }

    private static void ForcedUpdate()
    {
        DisunityHost.LoadWorld();
        var log = new List<string>();
        Canvas.WillRenderCanvases pre = () => log.Add("pre");
        Canvas.WillRenderCanvases bad = () => throw new InvalidOperationException("handler failed");
        Canvas.WillRenderCanvases will = () => log.Add("will");
        try
        {
            Canvas.preWillRenderCanvases += pre;
            Canvas.willRenderCanvases += bad;
            Canvas.willRenderCanvases += will;
            var before = DisunityHost.ScriptExceptionCount;
            Canvas.ForceUpdateCanvases();
            Expect.Sequence(new[] { "pre", "will" }, log, "forced update");
            Expect.Equal(before + 1, DisunityHost.ScriptExceptionCount, "the handler failure is reported once");
        }
        finally
        {
            Canvas.preWillRenderCanvases -= pre;
            Canvas.willRenderCanvases -= bad;
            Canvas.willRenderCanvases -= will;
            DisunityHost.UnloadWorld();
        }
    }
}
