using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Camera.Render and Camera.RenderWithShader complete without drawing until the native draw surface exists.</summary>
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
            NotesOncePerWorld();
            Console.WriteLine("PASS: Camera.Render logs its note once per world and again after a reload");
            RenderWithShaderValidates();
            Console.WriteLine("PASS: Camera.RenderWithShader validates the shader, returns and notes once");
            DestroyedCameraThrows();
            Console.WriteLine("PASS: Camera.Render and RenderWithShader throw on a destroyed camera");
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

    private static void NotesOncePerWorld()
    {
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        var camera = NewCamera();
        using (var log = new LogRecorder())
        {
            camera.Render();
            camera.Render();
            Ensure(Notes(log, "Camera.Render") == 1, $"one note across two calls: {string.Join("|", log.Messages)}");
        }
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        var reloaded = NewCamera();
        using (var log = new LogRecorder())
        {
            reloaded.Render();
            reloaded.Render();
            Ensure(Notes(log, "Camera.Render") == 1, $"one note again after a reload: {string.Join("|", log.Messages)}");
        }
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

    private static bool Catches(Action call)
    {
        try { call(); }
        catch (Exception) { return true; }
        return false;
    }
}
