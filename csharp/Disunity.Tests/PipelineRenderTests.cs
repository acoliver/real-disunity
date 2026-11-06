using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>The host calls the current pipeline's Render once per frame step, after the script phases, with the enabled
/// cameras in depth order; a Camera.Render while a pipeline runs renders through it; a throwing Render is reported once per
/// distinct exception and given up after three failing frames.</summary>
internal static class PipelineRenderTests
{
    private static readonly List<string> Events = new();

    private sealed class Probe : RenderPipeline
    {
        public readonly List<List<Camera>> Calls = new();
        public Func<int, Exception?> Fail = _ => null;
        public Action<List<Camera>>? During;

        protected override void Render(ScriptableRenderContext context, List<Camera> cameras)
        {
            Events.Add("render");
            Calls.Add(new List<Camera>(cameras));
            During?.Invoke(cameras);
            if (Fail(Calls.Count) is { } error) throw error;
        }
    }

    private sealed class ProbeAsset : RenderPipelineAsset
    {
        public readonly Probe Pipeline = new();

        protected override RenderPipeline CreatePipeline() => Pipeline;
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            RendersOncePerStepInDepthOrder();
            Console.WriteLine("PASS: a step calls the pipeline's Render once with the enabled, active cameras by depth and creation order");
            ContextEventsSurroundRender();
            Console.WriteLine("PASS: beginContextRendering and endContextRendering fire around Render with its cameras");
            NoPipelineMeansNoRender();
            Console.WriteLine("PASS: a step without a pipeline renders nothing and Camera.Render still records a CameraRender");
            CameraRenderGoesThroughThePipeline();
            Console.WriteLine("PASS: Camera.Render with a pipeline calls Render with that camera and records no CameraRender");
            NestedRenderIsRefused();
            Console.WriteLine("PASS: a camera rendered from inside Render throws InvalidOperationException");
            ThrowingRenderIsReportedOncePerDistinctException();
            Console.WriteLine("PASS: a throwing Render is reported once per distinct exception, and a fixed one shows next frame");
            ThreeFailingFramesStopTheCalls();
            Console.WriteLine("PASS: three consecutive frames failing the same way stop the calls and report that they stopped");
            ANewPipelineStartsClean();
            Console.WriteLine("PASS: a new pipeline is called again after an earlier one was given up");
        }
        finally
        {
            DisunityHost.SetRenderPipeline(null, null);
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera(float depth, bool enabled = true)
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.depth = depth;
        camera.enabled = enabled;
        return camera;
    }

    private static Probe Install()
    {
        var asset = ScriptableObject.CreateInstance<ProbeAsset>();
        DisunityHost.SetRenderPipeline(asset, null);
        DisunityHost.Step(0.016f);
        return asset.Pipeline;
    }

    private static void Remove()
    {
        DisunityHost.SetRenderPipeline(null, null);
        DisunityHost.Step(0.016f);
        Events.Clear();
    }

    private static void DestroyCameras(params Camera[] cameras)
    {
        foreach (var camera in cameras) UnityEngine.Object.DestroyImmediate(camera.gameObject);
    }

    private static void RendersOncePerStepInDepthOrder()
    {
        var late = NewCamera(5);
        var early = NewCamera(-1);
        var tie = NewCamera(5);
        var off = NewCamera(0, enabled: false);
        var inactive = NewCamera(1);
        inactive.gameObject.SetActive(false);
        var pipeline = Install();
        Expect.Equal(1, pipeline.Calls.Count, "the step that creates the pipeline also renders through it");
        Expect.Sequence(new[] { early, late, tie }, pipeline.Calls[0], "enabled active cameras by depth, ties by creation");
        DisunityHost.Step(0.016f);
        DisunityHost.Step(0.016f);
        Expect.Equal(3, pipeline.Calls.Count, "one Render per step");
        early.enabled = false;
        late.depth = -3;
        DisunityHost.Step(0.016f);
        Expect.Sequence(new[] { late, tie }, pipeline.Calls[3], "a disabled camera drops out and a changed depth reorders");
        Remove();
        DestroyCameras(late, early, tie, off, inactive);
        DisunityHost.Step(0.016f);
        var emptied = Install();
        Expect.Equal(0, emptied.Calls[0].Count, "a step with no cameras still calls Render with an empty list");
        Remove();
    }

    private static void ContextEventsSurroundRender()
    {
        var camera = NewCamera(0);
        var pipeline = Install();
        Events.Clear();
        List<Camera>? begun = null, ended = null;
        Action<ScriptableRenderContext, List<Camera>> begin = (_, cameras) => { Events.Add("begin"); begun = cameras; };
        Action<ScriptableRenderContext, List<Camera>> end = (_, cameras) => { Events.Add("end"); ended = cameras; };
        RenderPipelineManager.beginContextRendering += begin;
        RenderPipelineManager.endContextRendering += end;
        try { DisunityHost.Step(0.016f); }
        finally
        {
            RenderPipelineManager.beginContextRendering -= begin;
            RenderPipelineManager.endContextRendering -= end;
        }
        Expect.Sequence(new[] { "begin", "render", "end" }, Events, "events surround Render in order");
        Expect.Sequence(new[] { camera }, begun!, "begin gets the cameras");
        Expect.Sequence(new[] { camera }, ended!, "end gets the cameras");
        Remove();
        DestroyCameras(camera);
    }

    private static void NoPipelineMeansNoRender()
    {
        var camera = NewCamera(0);
        var target = new RenderTexture(16, 16, 0);
        camera.targetTexture = target;
        CommandWireReader.Take();
        DisunityHost.Step(0.016f);
        camera.Render();
        Expect.Sequence(new[] { "Flat0" }, CommandWireReader.Take(), "without a pipeline Camera.Render records a CameraRender");
        DestroyCameras(camera);
    }

    private static void CameraRenderGoesThroughThePipeline()
    {
        var camera = NewCamera(0);
        var other = NewCamera(1);
        var target = new RenderTexture(16, 16, 0);
        camera.targetTexture = target;
        var pipeline = Install();
        CommandWireReader.Take();
        pipeline.Calls.Clear();
        camera.Render();
        Expect.Equal(1, pipeline.Calls.Count, "Camera.Render calls Render once");
        Expect.Sequence(new[] { camera }, pipeline.Calls[0], "with only that camera, whatever other cameras exist");
        Expect.Equal(0, CommandWireReader.Take().Length, "no CameraRender is recorded");
        other.Render();
        Expect.Sequence(new[] { other }, pipeline.Calls[1], "a camera without a target texture renders through it too");
        Remove();
        DestroyCameras(camera, other);
    }

    private static void NestedRenderIsRefused()
    {
        var camera = NewCamera(0);
        var pipeline = Install();
        pipeline.Calls.Clear();
        Exception? nested = null;
        pipeline.During = _ =>
        {
            try { camera.Render(); }
            catch (Exception error) { nested = error; }
        };
        DisunityHost.Step(0.016f);
        Expect.That(nested is InvalidOperationException, $"the nested render throws InvalidOperationException, got {nested?.GetType().Name ?? "nothing"}");
        Expect.Equal(1, pipeline.Calls.Count, "the nested render never reached Render");
        pipeline.During = null;
        camera.Render();
        Expect.Equal(2, pipeline.Calls.Count, "after the outer Render ends a camera renders again");
        Remove();
        DestroyCameras(camera);
    }

    private static void ThrowingRenderIsReportedOncePerDistinctException()
    {
        var pipeline = Install();
        var before = DisunityHost.ScriptExceptionCount;
        pipeline.Calls.Clear();
        pipeline.Fail = call => call switch
        {
            1 or 2 => new InvalidOperationException("first problem"),
            3 => new InvalidOperationException("second problem"),
            _ => null,
        };
        using var log = new LogRecorder();
        for (var step = 0; step < 3; step++) DisunityHost.Step(0.016f);
        Expect.Equal(2, DisunityHost.ScriptExceptionCount - before, "the same exception twice is reported once, a different one again");
        Expect.Equal(3, pipeline.Calls.Count, "Render keeps being called while it fails");
        DisunityHost.Step(0.016f);
        DisunityHost.Step(0.016f);
        Expect.Equal(5, pipeline.Calls.Count, "a frame that renders cleanly keeps the pipeline going");
        Expect.Equal(2, DisunityHost.ScriptExceptionCount - before, "and reports nothing");
        Expect.That(!log.Messages.Any(m => m.Contains("stopped")), "no stop was reported");
        Remove();
    }

    private static void ThreeFailingFramesStopTheCalls()
    {
        var pipeline = Install();
        var before = DisunityHost.ScriptExceptionCount;
        pipeline.Calls.Clear();
        var broken = true;
        pipeline.Fail = _ => broken ? new InvalidOperationException("same problem") : null;
        using var log = new LogRecorder();
        DisunityHost.Step(0.016f);
        DisunityHost.Step(0.016f);
        broken = false;
        DisunityHost.Step(0.016f);
        broken = true;
        for (var step = 0; step < 3; step++) DisunityHost.Step(0.016f);
        Expect.Equal(6, pipeline.Calls.Count, "a clean frame resets the count of failing frames");
        for (var step = 0; step < 4; step++) DisunityHost.Step(0.016f);
        Expect.Equal(6, pipeline.Calls.Count, "after the third consecutive failure Render is not called again");
        Expect.Equal(1, log.Messages.Count(m => m.StartsWith("Error:") && m.Contains("stopped")), "the stop is reported once");
        Expect.That(DisunityHost.ScriptExceptionCount - before >= 1, "the exception itself was reported");
        Expect.That(RenderPipelineManager.currentPipeline == pipeline && !pipeline.disposed, "the pipeline stays current and undisposed");
    }

    private static void ANewPipelineStartsClean()
    {
        Remove();
        var fresh = Install();
        Expect.Equal(1, fresh.Calls.Count, "a new pipeline is rendered through from its first step");
        DisunityHost.Step(0.016f);
        Expect.Equal(2, fresh.Calls.Count, "and keeps being rendered through");
        Remove();
    }
}
