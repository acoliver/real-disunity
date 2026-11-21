using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Camera.Render with a target records offscreen work and without one refuses to draw to the screen;
/// Camera.RenderWithShader completes without drawing until the native draw surface exists; Camera.SetTargetBuffers gives the
/// camera the buffers it draws into.</summary>
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
            SetTargetBuffersRecordsTheBuffers();
            Console.WriteLine("PASS: Camera.SetTargetBuffers records the color and depth buffers of both overloads, replaces targetTexture and is replaced by it");
            SetTargetBuffersRefusesWhatItCannotDrawInto();
            Console.WriteLine("PASS: Camera.SetTargetBuffers refuses buffers a pass cannot draw into, by name, and keeps the buffers it had");
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

    private static WorldGraphics.CameraBuffers? Buffers(Camera camera) => WorldGraphics.ReadCameraBuffers(camera.Handle);

    private static void BuffersAre(Camera camera, RenderTexture[] colors, RenderTexture? depth, string what)
    {
        var held = Buffers(camera);
        Ensure(held is not null, $"{what}: the camera has buffers");
        Expect.Sequence(colors.Select(color => color.WorldId), held!.Value.Colors, $"{what}: the color textures in order");
        Expect.Equal(depth?.WorldId ?? 0UL, held.Value.Depth, $"{what}: the depth texture");
    }

    /// <summary>Both overloads give the world the color buffers in order and the depth buffer, which replace the target texture, and
    /// setting the target texture replaces them again.</summary>
    private static void SetTargetBuffersRecordsTheBuffers()
    {
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        var camera = NewCamera();
        var target = new RenderTexture(16, 16, 0);
        var colors = new[] { new RenderTexture(16, 16, 0), new RenderTexture(16, 16, 0), new RenderTexture(16, 16, 0) };
        var depth = new RenderTexture(16, 16, 24);
        Expect.Throws<ArgumentNullException>(() => camera.SetTargetBuffers((RenderBuffer[])null!, depth.depthBuffer), "null color buffers");
        Ensure(Buffers(camera) is null, "a refused call sets nothing");
        using var log = new LogRecorder();

        camera.targetTexture = target;
        camera.SetTargetBuffers(colors[0].colorBuffer, depth.depthBuffer);
        BuffersAre(camera, [colors[0]], depth, "the single overload");
        Ensure(camera.targetTexture == null && camera.activeTexture == null, "the buffers replace targetTexture");

        camera.SetTargetBuffers(colors.Select(color => color.colorBuffer).ToArray(), depth.depthBuffer);
        BuffersAre(camera, colors, depth, "the array overload");
        Ensure(colors.All(color => color.IsCreated()) && depth.IsCreated(), "the textures are created in the world");

        camera.targetTexture = target;
        Ensure(Buffers(camera) is null && camera.targetTexture == target, "a target texture replaces the buffers");

        camera.SetTargetBuffers(colors.Select(color => color.colorBuffer).ToArray(), default);
        BuffersAre(camera, colors, null, "no depth buffer");
        camera.targetTexture = null;
        Ensure(Buffers(camera) is null && camera.targetTexture == null, "a null target texture removes the buffers");

        camera.SetTargetBuffers(colors.Select(color => color.colorBuffer).ToArray(), depth.depthBuffer);
        camera.enabled = false;
        camera.depth = 3;
        BuffersAre(camera, colors, depth, "the buffers survive other settings");
        Ensure(Notes(log, "Camera.SetTargetBuffers") == 0, $"no note of an undrawn call: {string.Join("|", log.Messages)}");

        var copy = NewCamera();
        copy.CopyFrom(camera);
        BuffersAre(copy, colors, depth, "CopyFrom");
        var other = NewCamera();
        other.targetTexture = target;
        copy.CopyFrom(other);
        Ensure(Buffers(copy) is null && copy.targetTexture == target, "CopyFrom of a camera with a target texture replaces the buffers");

        var error = Expect.Catch<NotSupportedException>(() => camera.Render(), "a render of a camera with buffers outside a pipeline");
        Ensure(error.Message.Contains("SetTargetBuffers"), $"the message names the call: {error.Message}");
        UnityEngine.Object.DestroyImmediate(camera);
        Expect.That(Catches(() => camera.SetTargetBuffers(colors[0].colorBuffer, depth.depthBuffer)), "SetTargetBuffers on a destroyed camera");
    }

    /// <summary>The engine refuses what a pass cannot draw into, by name, and a refused call leaves the buffers as they were.</summary>
    private static void SetTargetBuffersRefusesWhatItCannotDrawInto()
    {
        var camera = NewCamera();
        var color = new RenderTexture(16, 16, 0);
        var depth = new RenderTexture(16, 16, 24);
        camera.SetTargetBuffers(color.colorBuffer, depth.depthBuffer);
        var many = Enumerable.Range(0, 9).Select(_ => new RenderTexture(16, 16, 0)).ToArray();
        var nine = many.Select(texture => texture.colorBuffer).ToArray();
        var narrow = new RenderTexture(8, 16, 0);
        var short_ = new RenderTexture(16, 8, 24);
        var destroyed = new RenderTexture(16, 16, 0);
        var destroyedBuffer = destroyed.colorBuffer;
        UnityEngine.Object.DestroyImmediate(destroyed);
        var refused = new (string What, Action Call)[]
        {
            ("no color buffers", () => camera.SetTargetBuffers(Array.Empty<RenderBuffer>(), depth.depthBuffer)),
            ("nine color buffers", () => camera.SetTargetBuffers(nine, depth.depthBuffer)),
            ("one texture twice", () => camera.SetTargetBuffers([color.colorBuffer, color.colorBuffer], depth.depthBuffer)),
            ("a depth buffer as a color buffer", () => camera.SetTargetBuffers(depth.depthBuffer, depth.depthBuffer)),
            ("a color buffer as the depth buffer", () => camera.SetTargetBuffers(color.colorBuffer, color.colorBuffer)),
            ("no color buffer", () => camera.SetTargetBuffers(default(RenderBuffer), depth.depthBuffer)),
            ("a destroyed texture", () => camera.SetTargetBuffers(destroyedBuffer, depth.depthBuffer)),
            ("a color buffer of another size", () => camera.SetTargetBuffers([color.colorBuffer, narrow.colorBuffer], depth.depthBuffer)),
            ("a depth buffer of another size", () => camera.SetTargetBuffers(color.colorBuffer, short_.depthBuffer)),
        };
        foreach (var (what, call) in refused)
        {
            Expect.Throws<ArgumentException>(call, what);
            BuffersAre(camera, [color], depth, $"{what} leaves the buffers");
        }
        GC.KeepAlive(many);
    }

    private static bool Catches(Action call)
    {
        try { call(); }
        catch (Exception) { return true; }
        return false;
    }
}
