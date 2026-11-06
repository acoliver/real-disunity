using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>ScriptableRenderContext.SetupCameraProperties records the camera's view, projection and target when it is called,
/// and sends the record at Submit in order with the commands around it.</summary>
internal static class SetupCameraTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            SetupRecordsTheCameraAsItIsNow();
            Console.WriteLine("PASS: SetupCameraProperties queues the camera's view, projection and target as they are when it is called");
            SetupFollowsTheCommandsAroundIt();
            Console.WriteLine("PASS: a camera setup is sent at Submit in order with the command buffers executed before and after it");
            SetupRefusesWhatItCannotRecord();
            Console.WriteLine("PASS: SetupCameraProperties refuses a null or destroyed camera and stereo setup");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.orthographic = false;
        camera.fieldOfView = 50f;
        return camera;
    }

    private static string Matrix(Matrix4x4 m)
    {
        var values = new List<string>();
        for (var column = 0; column < 4; column++)
            for (var row = 0; row < 4; row++) values.Add(m[row, column].ToString("R", System.Globalization.CultureInfo.InvariantCulture));
        return "(" + string.Join(" ", values) + ")";
    }

    private static void SetupRecordsTheCameraAsItIsNow()
    {
        var camera = NewCamera();
        camera.transform.position = new Vector3(2f, -1f, 4f);
        var context = default(ScriptableRenderContext);
        CommandWireReader.Take();
        context.SetupCameraProperties(camera);
        var view = camera.worldToCameraMatrix;
        camera.transform.position = new Vector3(9f, 9f, 9f);
        Expect.Equal(0, CommandWireReader.Take().Length, "nothing reaches the world before Submit");
        context.Submit();
        var taken = CommandWireReader.Take();
        Expect.Equal(1, taken.Length, "one setup record");
        var expected = $"view={Matrix(view)} projection={Matrix(camera.projectionMatrix)} target=Camera";
        Expect.That(taken[0].EndsWith(expected, StringComparison.Ordinal) || taken[0].Contains("view=" + Matrix(view)), $"the camera as it was when called: {taken[0]}");
        Expect.That(taken[0].Contains("projection=" + Matrix(camera.projectionMatrix)) && taken[0].EndsWith("target=Camera", StringComparison.Ordinal), $"projection and screen target: {taken[0]}");
        var target = new RenderTexture(16, 16, 0);
        camera.targetTexture = target;
        context.SetupCameraProperties(camera, false);
        context.Submit();
        var withTexture = CommandWireReader.Take();
        Expect.That(withTexture[0].EndsWith($"target=Render:{target.WorldId}", StringComparison.Ordinal), $"a target texture is the target: {withTexture[0]}");
    }

    private static void SetupFollowsTheCommandsAroundIt()
    {
        var camera = NewCamera();
        var context = default(ScriptableRenderContext);
        var before = new CommandBuffer();
        before.EnableShaderKeyword("BEFORE");
        var after = new CommandBuffer();
        after.EnableShaderKeyword("AFTER");
        CommandWireReader.Take();
        context.ExecuteCommandBuffer(before);
        context.SetupCameraProperties(camera);
        context.ExecuteCommandBuffer(after);
        context.Submit();
        var taken = CommandWireReader.Take();
        Expect.Equal(3, taken.Length, "three records");
        Expect.That(taken[0] == "Keyword BEFORE=True" && taken[1].StartsWith("SetupCamera ", StringComparison.Ordinal) && taken[2] == "Keyword AFTER=True", $"in call order: {string.Join(" | ", taken.Select(t => t[..Math.Min(20, t.Length)]))}");
    }

    private static void SetupRefusesWhatItCannotRecord()
    {
        var context = default(ScriptableRenderContext);
        var camera = NewCamera();
        Expect.Throws<ArgumentNullException>(() => context.SetupCameraProperties(null!), "a null camera");
        var error = Expect.Catch<NotSupportedException>(() => context.SetupCameraProperties(camera, true), "stereo setup");
        Expect.That(error.Message.Contains("ScriptableRenderContext.SetupCameraProperties", StringComparison.Ordinal), $"the message names the member: {error.Message}");
        var destroyed = NewCamera();
        UnityEngine.Object.DestroyImmediate(destroyed);
        Expect.Throws<InvalidOperationException>(() => context.SetupCameraProperties(destroyed), "a destroyed camera");
        context.Submit();
        CommandWireReader.Take();
    }
}
