using Disunity.Hosting;
using UnityEngine;
using UnityEngine.UI;

internal static class PresentationTests
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
            TextStyleRoundTrip();
            Console.WriteLine("PASS: native camera clear and lens settings, canvas screen-size scaling, text style");
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

    private static void TextStyleRoundTrip()
    {
        var font = DisunityHost.RegisterFont(SyntheticFont.Bytes());
        var text = DisunityHost.CreateText(DisunityHost.CreateGameObject(), font);
        Require(text.font == font, "text keeps its font");
        Require(text.alignment == TextAnchor.UpperLeft && text.supportRichText, "Unity text defaults");
        Require(text.horizontalOverflow == HorizontalWrapMode.Wrap && text.verticalOverflow == VerticalWrapMode.Truncate, "Unity overflow defaults");
        text.supportRichText = false;
        text.alignment = TextAnchor.MiddleCenter;
        text.color = new Color(1, 1, 1, 0.5f);
        text.lineSpacing = 1.25f;
        text.verticalOverflow = VerticalWrapMode.Overflow;
        Require(text.alignment == TextAnchor.MiddleCenter && !text.supportRichText, "alignment and rich text");
        Require(text.color == new Color(1, 1, 1, 0.5f) && text.lineSpacing == 1.25f, "colour and line spacing");
        Require(text.verticalOverflow == VerticalWrapMode.Overflow, "vertical overflow");
        Rejects<DisunityException>(() => text.fontStyle = FontStyle.Bold, "bold is unsupported");
        Rejects<DisunityException>(() => text.resizeTextForBestFit = true, "best fit is unsupported");
        Rejects<DisunityException>(() => text.lineSpacing = 0, "zero line spacing");
        Require(text.fontStyle == FontStyle.Normal && !text.resizeTextForBestFit && text.lineSpacing == 1.25f,
            "rejected writes leave the style unchanged");
        text.text = "<b>7</b>";
        Rejects<DisunityException>(() => text.supportRichText = true, "markup needs rich text rendering");
        Require(!text.supportRichText && text.text == "<b>7</b>", "markup stays literal");
        UnityEngine.Object.Destroy(text.gameObject);
        DisunityHost.Step(0.02f);
    }
}
