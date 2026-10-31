using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Camera lens setters that clamp what scripts commonly assign instead of failing the frame.</summary>
internal static class CameraClampTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            FieldOfViewClamps();
            FieldOfViewKeepsRejectingNaN();
            ClipPlanesClamp();
            OtherSettersKeepRejecting();
            Console.WriteLine("PASS: Camera field of view and clip plane clamping");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera() => DisunityHost.CreateGameObject().AddComponent<Camera>();

    private static void FieldOfViewClamps()
    {
        var camera = NewCamera();
        camera.fieldOfView = 200f;
        Ensure(camera.fieldOfView == 179f, $"field of view above the range clamps to 179, got {camera.fieldOfView}");
        camera.fieldOfView = 180f;
        Ensure(camera.fieldOfView == 179f, "a field of view of 180 clamps to 179");
        camera.fieldOfView = 0f;
        Ensure(camera.fieldOfView == 0.00001f, $"a field of view of 0 clamps to the minimum, got {camera.fieldOfView}");
        camera.fieldOfView = -5f;
        Ensure(camera.fieldOfView == 0.00001f, "a negative field of view clamps to the minimum");
        camera.fieldOfView = float.PositiveInfinity;
        Ensure(camera.fieldOfView == 179f, "an infinite field of view clamps to 179");
        camera.fieldOfView = 90f;
        Ensure(camera.fieldOfView == 90f, "a field of view inside the range is kept");
    }

    private static void FieldOfViewKeepsRejectingNaN()
    {
        var camera = NewCamera();
        camera.fieldOfView = 70f;
        Throws<DisunityException>(() => camera.fieldOfView = float.NaN, "NaN field of view");
        Ensure(camera.fieldOfView == 70f, "a rejected field of view changes nothing");
    }

    private static void ClipPlanesClamp()
    {
        var camera = NewCamera();
        camera.nearClipPlane = -1f;
        Ensure(camera.nearClipPlane == 0.01f, $"a negative near plane clamps to 0.01, got {camera.nearClipPlane}");
        camera.nearClipPlane = 0f;
        Ensure(camera.nearClipPlane == 0.01f, "a zero near plane clamps to 0.01");
        camera.nearClipPlane = 0.3f;
        camera.farClipPlane = 0.1f;
        Ensure(camera.farClipPlane == 0.3f + 0.01f, $"a far plane before the near plane clamps to near + 0.01, got {camera.farClipPlane}");
        camera.farClipPlane = 0.3f;
        Ensure(camera.farClipPlane == 0.3f + 0.01f, "a far plane equal to the near plane clamps to near + 0.01");
        camera.farClipPlane = 1000f;
        Ensure(camera.farClipPlane == 1000f && camera.nearClipPlane == 0.3f, "clip planes inside the range are kept");
        Throws<DisunityException>(() => camera.nearClipPlane = float.NaN, "NaN near plane");
        Throws<DisunityException>(() => camera.farClipPlane = float.NaN, "NaN far plane");
    }

    // The engine's choice: sizes and rectangles that a script sets to nonsense are errors, not clamped.
    private static void OtherSettersKeepRejecting()
    {
        var camera = NewCamera();
        camera.orthographicSize = 4f;
        Throws<DisunityException>(() => camera.orthographicSize = 0f, "zero orthographic size");
        Throws<DisunityException>(() => camera.orthographicSize = -1f, "negative orthographic size");
        Throws<ArgumentOutOfRangeException>(() => camera.aspect = -1f, "negative aspect", "aspect");
        Throws<DisunityException>(() => camera.rect = new Rect(0, 0, -1, 1), "negative rect width");
        Ensure(camera.orthographicSize == 4f && camera.rect == new Rect(0, 0, 1, 1), "rejected writes change nothing");
    }
}
