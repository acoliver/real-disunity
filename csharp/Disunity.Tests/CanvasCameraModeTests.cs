using Disunity.Hosting;
using UnityEngine;

/// <summary>A canvas switched to ScreenSpaceCamera before it has a camera draws as an overlay until it gets one.</summary>
internal static class CanvasCameraModeTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ModeBeforeCamera();
            CameraBeforeMode();
            LeavingTheMode();
            AlreadyInCameraMode();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: a ScreenSpaceCamera canvas without a camera draws as an overlay until worldCamera is set");
    }

    private static Canvas NewCanvas()
    {
        var owner = DisunityHost.CreateGameObject();
        owner.AddComponent<RectTransform>();
        return owner.AddComponent<Canvas>();
    }

    private static Camera NewCamera() => DisunityHost.CreateGameObject().AddComponent<Camera>();

    private static void ModeBeforeCamera()
    {
        var canvas = NewCanvas();
        canvas.renderMode = RenderMode.ScreenSpaceCamera;
        Expect.Equal(RenderMode.ScreenSpaceCamera, canvas.renderMode, "the requested mode is reported");
        Expect.That(canvas.worldCamera == null, "no camera yet");
        var camera = NewCamera();
        canvas.worldCamera = camera;
        Expect.Equal(RenderMode.ScreenSpaceCamera, canvas.renderMode, "still camera mode");
        Expect.That(ReferenceEquals(canvas.worldCamera, camera), "the camera renders the canvas");
        Expect.Near(100f, canvas.planeDistance, "the default plane distance");
    }

    private static void CameraBeforeMode()
    {
        var canvas = NewCanvas();
        var camera = NewCamera();
        canvas.worldCamera = camera;
        Expect.Equal(RenderMode.ScreenSpaceOverlay, canvas.renderMode, "a stored camera does not change the mode");
        canvas.renderMode = RenderMode.ScreenSpaceCamera;
        Expect.That(ReferenceEquals(canvas.worldCamera, camera), "the stored camera now renders the canvas");
        Expect.Equal(RenderMode.ScreenSpaceCamera, canvas.renderMode, "camera mode");
        var placed = NewCanvas();
        placed.planeDistance = 40f;
        placed.worldCamera = NewCamera();
        placed.renderMode = RenderMode.ScreenSpaceCamera;
        Expect.Near(40f, placed.planeDistance, "a canvas keeps its own plane distance when it enters camera mode");
    }

    private static void LeavingTheMode()
    {
        var canvas = NewCanvas();
        canvas.renderMode = RenderMode.ScreenSpaceCamera;
        canvas.renderMode = RenderMode.WorldSpace;
        Expect.Equal(RenderMode.WorldSpace, canvas.renderMode, "the later mode wins");
        var camera = NewCamera();
        canvas.worldCamera = camera;
        Expect.Equal(RenderMode.WorldSpace, canvas.renderMode, "a camera set afterwards is only an event camera");
        canvas.renderMode = RenderMode.ScreenSpaceOverlay;
        Expect.Equal(RenderMode.ScreenSpaceOverlay, canvas.renderMode, "overlay");
    }

    private static void AlreadyInCameraMode()
    {
        var canvas = NewCanvas();
        var camera = NewCamera();
        DisunityHost.RenderThroughCamera(canvas, camera, 25f);
        canvas.renderMode = RenderMode.ScreenSpaceCamera;
        Expect.That(ReferenceEquals(canvas.worldCamera, camera), "setting the same mode keeps the camera");
        Expect.Near(25f, canvas.planeDistance, "and its plane distance");
        canvas.renderMode = RenderMode.ScreenSpaceOverlay;
        Expect.Near(25f, canvas.planeDistance, "leaving camera mode keeps the plane distance");
        canvas.planeDistance = 60f;
        canvas.renderMode = RenderMode.ScreenSpaceCamera;
        Expect.Near(60f, canvas.planeDistance, "a distance set outside camera mode is used when it returns");
    }
}
