using Disunity.Hosting;
using UnityEngine;

internal static class CameraTests
{
    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void Rejects<T>(Action action, string message) where T : Exception
    {
        try { action(); }
        catch (T) { return; }
        throw new InvalidOperationException(message);
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            CameraSettingsRoundTrip();
            CanvasScaleFollowsTheViewport();
            Console.WriteLine("PASS: native camera clear and lens settings, canvas screen-size scaling");
        }
        finally
        {
            DisunityHost.SetViewport(800, 600);
            DisunityHost.UnloadWorld();
        }
    }

    private static void CameraSettingsRoundTrip()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        Require(camera.clearFlags == CameraClearFlags.SolidColor && camera.orthographic, "default camera");
        camera.clearFlags = CameraClearFlags.Skybox;
        camera.backgroundColor = new Color(0.25f, 0.5f, 0.75f, 0);
        camera.orthographicSize = 8;
        camera.nearClipPlane = 0.3f;
        camera.farClipPlane = 1000;
        camera.depth = -1;
        Require(camera.clearFlags == CameraClearFlags.Skybox, "clear flags");
        Require(camera.backgroundColor == new Color(0.25f, 0.5f, 0.75f, 0), "background colour");
        Require(camera.orthographicSize == 8 && camera.depth == -1, "lens and depth");
        Require(camera.nearClipPlane == 0.3f && camera.farClipPlane == 1000, "clip planes");
        Rejects<DisunityException>(() => camera.orthographic = false, "perspective is unsupported");
        Rejects<DisunityException>(() => camera.orthographicSize = 0, "zero orthographic size");
        Rejects<DisunityException>(() => camera.farClipPlane = 0.1f, "far plane before near plane");
        Rejects<ArgumentException>(() => camera.depth = 0.5f, "fractional depth");
        Require(camera.orthographic && camera.orthographicSize == 8 && camera.farClipPlane == 1000 && camera.depth == -1,
            "rejected writes leave the camera unchanged");
        camera.enabled = false;
        Require(!camera.enabled, "disabled camera");
        UnityEngine.Object.Destroy(camera.gameObject);
        DisunityHost.Step(0.02f);
    }

    private static void CanvasScaleFollowsTheViewport()
    {
        var canvas = DisunityHost.CreateGameObject().AddComponent<Canvas>();
        Require(canvas.scaleFactor == 1, "default canvas scale");
        DisunityHost.ScaleWithScreenSize(canvas, new Vector2(1920, 1080), 0);
        DisunityHost.SetViewport(960, 540);
        Require(canvas.scaleFactor == 0.5f, $"half-size screen scales by 0.5, got {canvas.scaleFactor}");
        DisunityHost.SetViewport(1920, 1080);
        Require(canvas.scaleFactor == 1, "reference screen scales by 1");
        canvas.sortingOrder = 3;
        DisunityHost.SetViewport(960, 540);
        Require(canvas.scaleFactor == 0.5f, "canvas writes keep screen scaling");
        DisunityHost.ScaleWithScreenSize(canvas, new Vector2(1920, 1080), 1);
        DisunityHost.SetViewport(960, 1080);
        Require(canvas.scaleFactor == 1, "matching height ignores width");
        Rejects<DisunityException>(() => DisunityHost.ScaleWithScreenSize(canvas, new Vector2(0, 1080), 0), "zero reference");
        Require(canvas.scaleFactor == 1, "rejected scaling leaves the canvas unchanged");
        canvas.scaleFactor = 2;
        Require(canvas.scaleFactor == 2, "setting scaleFactor gives a constant factor");
        UnityEngine.Object.Destroy(canvas.gameObject);
        DisunityHost.Step(0.02f);
    }
}
