using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Camera.Render with a target records offscreen work and without one refuses to draw to the screen;
/// Camera.RenderWithShader completes without drawing until the native draw surface exists.</summary>
internal static class CameraRenderTests
{
    private const string Note = "draws nothing yet: the engine has no draw surface";

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            RenderLeavesTargetsAlone();
            Console.WriteLine("PASS: Camera.Render returns and leaves targetTexture and RenderTexture.active unchanged");
            ScreenRenderThrows();
            Console.WriteLine("PASS: Camera.Render without a target throws NotSupportedException every time, records nothing and leaves the camera alone");
            RenderWithShaderValidates();
            Console.WriteLine("PASS: Camera.RenderWithShader validates the shader, returns and notes once");
            DestroyedCameraThrows();
            Console.WriteLine("PASS: Camera.Render and RenderWithShader throw on a destroyed camera");
            SetTargetBuffersReturns();
            Console.WriteLine("PASS: Camera.SetTargetBuffers validates, returns, leaves targetTexture unchanged and notes once");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera() => DisunityHost.CreateGameObject().AddComponent<Camera>();

    private static int Notes(GraphicsTestSupport.LogRecorder log, string member) =>
        log.Messages.Count(m => m == $"Warning: {member} {Note}");

    private static void RenderLeavesTargetsAlone()
    {
        var camera = NewCamera();
        var target = new RenderTexture(16, 16, 0);
        var other = new RenderTexture(8, 8, 0);
        camera.targetTexture = target;
        RenderTexture.active = other;
        camera.Render();
        Ensure(camera.targetTexture == target, "targetTexture is unchanged");
        Ensure(RenderTexture.active == other, "RenderTexture.active is unchanged");
        Ensure(camera.enabled, "the camera is still enabled");
    }

    /// <summary>The screen is drawn by the frame from the selected camera, so an explicit screen render has nothing to draw
    /// into: it throws on every call, for an enabled or a disabled camera and again after a reload, and leaves no note.</summary>
    private static void ScreenRenderThrows()
    {
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        var camera = NewCamera();
        var active = new RenderTexture(8, 8, 0);
        RenderTexture.active = active;
        using (var log = new LogRecorder())
        {
            var error = Expect.Catch<NotSupportedException>(() => camera.Render(), "a screen render");
            Ensure(error.Message.Contains("Camera.Render") && error.Message.Contains("targetTexture"),
                $"the message names the call and the way to render: {error.Message}");
            Expect.Throws<NotSupportedException>(() => camera.Render(), "a second screen render");
            camera.enabled = false;
            Expect.Throws<NotSupportedException>(() => camera.Render(), "a disabled camera's screen render");
            Ensure(Notes(log, "Camera.Render") == 0, $"no note: {string.Join("|", log.Messages)}");
        }
        Ensure(camera.targetTexture == null && RenderTexture.active == active, "the camera and active texture are unchanged");
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        Expect.Throws<NotSupportedException>(() => NewCamera().Render(), "a screen render after a reload");
    }

    private static void RenderWithShaderValidates()
    {
        var camera = NewCamera();
        Expect.Throws<ArgumentNullException>(() => camera.RenderWithShader(null!, "RenderType"), "a null shader");
        var shader = DisunityHost.RegisterShader("Test/CameraRender");
        using var log = new LogRecorder();
        camera.RenderWithShader(shader, "RenderType");
        camera.RenderWithShader(shader, "");
        Ensure(Notes(log, "Camera.RenderWithShader") == 1, $"one note across two calls: {string.Join("|", log.Messages)}");
        Ensure(Notes(log, "Camera.Render") == 0, "the two members note separately");
    }

    private static void DestroyedCameraThrows()
    {
        var camera = NewCamera();
        var shader = DisunityHost.RegisterShader("Test/CameraRender");
        UnityEngine.Object.DestroyImmediate(camera);
        Expect.That(Catches(() => camera.Render()), "Render on a destroyed camera");
        Expect.That(Catches(() => camera.RenderWithShader(shader, "")), "RenderWithShader on a destroyed camera");
    }

    private static void SetTargetBuffersReturns()
    {
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        var camera = NewCamera();
        var target = new RenderTexture(16, 16, 0);
        camera.targetTexture = target;
        var depth = new RenderTexture(16, 16, 24).depthBuffer;
        Expect.Throws<ArgumentNullException>(() => camera.SetTargetBuffers((RenderBuffer[])null!, depth), "null color buffers");
        using var log = new LogRecorder();
        camera.SetTargetBuffers(target.colorBuffer, depth);
        camera.SetTargetBuffers(new[] { target.colorBuffer, target.colorBuffer }, depth);
        Ensure(camera.targetTexture == target, "targetTexture is unchanged");
        Ensure(Notes(log, "Camera.SetTargetBuffers") == 1, $"one note across both overloads: {string.Join("|", log.Messages)}");
        UnityEngine.Object.DestroyImmediate(camera);
        Expect.That(Catches(() => camera.SetTargetBuffers(target.colorBuffer, depth)), "SetTargetBuffers on a destroyed camera");
    }

    private static bool Catches(Action call)
    {
        try { call(); }
        catch (Exception) { return true; }
        return false;
    }
}
