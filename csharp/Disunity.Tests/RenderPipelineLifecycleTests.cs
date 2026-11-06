using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The host creates the active asset's pipeline on a frame step, replaces it when the active asset changes,
/// disposes it with the world, and reports a failing creation once.</summary>
internal static class RenderPipelineLifecycleTests
{
    private sealed class Pipeline : RenderPipeline
    {
        public int Disposals;
        public bool DisposedAsFinalizer;
        public static int Renders;

        protected override void Render(ScriptableRenderContext context, List<Camera> cameras) => Renders++;

        protected override void Dispose(bool disposing)
        {
            Disposals++;
            DisposedAsFinalizer |= !disposing;
            base.Dispose(disposing);
        }
    }

    private sealed class Asset : RenderPipelineAsset
    {
        public int Creations;
        public Pipeline? Created;

        protected override RenderPipeline CreatePipeline()
        {
            Creations++;
            return Created = new Pipeline();
        }
    }

    private sealed class FailingAsset : RenderPipelineAsset
    {
        public int Attempts;

        protected override RenderPipeline CreatePipeline()
        {
            Attempts++;
            throw new InvalidOperationException("pipeline resources are missing");
        }
    }

    private sealed class ListPipeline : RenderPipeline
    {
        public int ListCalls;
        public int ListLength;

        protected override void Render(ScriptableRenderContext context, List<Camera> cameras)
        {
            ListCalls++;
            ListLength = cameras.Count;
        }

        public void RenderArray(Camera[] cameras) => Render(default, cameras);
    }

    private static void SupportedFeaturesAreHeldAndNeverNull()
    {
        var original = SupportedRenderingFeatures.active;
        Expect.That(original is not null, "a default set of features is active at first");
        var replacement = new SupportedRenderingFeatures();
        SupportedRenderingFeatures.active = replacement;
        Expect.That(ReferenceEquals(SupportedRenderingFeatures.active, replacement), "a pipeline's features replace the active set");
        Expect.Throws<ArgumentNullException>(() => SupportedRenderingFeatures.active = null!, "the active set cannot be null");
        SupportedRenderingFeatures.active = original;
        var flags = typeof(SupportedRenderingFeatures).GetProperties().Where(p => p.PropertyType == typeof(bool) && !p.GetMethod!.IsStatic).ToArray();
        Expect.That(flags.Length >= 19, "the flags a pipeline sets exist");
        foreach (var flag in flags)
        {
            var features = new SupportedRenderingFeatures();
            var offByDefault = flag.Name.StartsWith("overrides", StringComparison.Ordinal) || flag.Name == "rendersUIOverlay";
            Expect.That((bool)flag.GetValue(features)! != offByDefault, $"{flag.Name} defaults to {!offByDefault}");
            var before = flags.Select(other => (bool)other.GetValue(features)!).ToArray();
            flag.SetValue(features, !before[Array.IndexOf(flags, flag)]);
            var after = flags.Select(other => (bool)other.GetValue(features)!).ToArray();
            Expect.That(after.Where((value, index) => value != before[index]).Count() == 1 && after[Array.IndexOf(flags, flag)] != before[Array.IndexOf(flags, flag)], $"{flag.Name} stores its own value");
        }
    }

    private static int _created;
    private static int _disposedEvents;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        RenderPipelineManager.activeRenderPipelineCreated += OnCreated;
        RenderPipelineManager.activeRenderPipelineDisposed += OnDisposed;
        try
        {
            AnActiveAssetCreatesItsPipelineOnce();
            ChangingTheAssetDisposesTheOldAndCreatesTheNew();
            ClearingTheAssetDisposesThePipeline();
            UnloadingTheWorldDisposesThePipeline();
            AFailingCreationIsReportedOnce();
            DisposeRunsOnceAndTheArrayFormRendersThroughTheListForm();
            SupportedFeaturesAreHeldAndNeverNull();
        }
        finally
        {
            RenderPipelineManager.activeRenderPipelineCreated -= OnCreated;
            RenderPipelineManager.activeRenderPipelineDisposed -= OnDisposed;
            DisunityHost.SetRenderPipeline(null, null);
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: the host creates the active asset's pipeline on a step, replaces and disposes it, and reports a failing creation once");
    }

    private static void OnCreated() => _created++;

    private static void OnDisposed() => _disposedEvents++;

    private static void Reset()
    {
        _created = 0;
        _disposedEvents = 0;
        Pipeline.Renders = 0;
    }

    private static void AnActiveAssetCreatesItsPipelineOnce()
    {
        Reset();
        var asset = ScriptableObject.CreateInstance<Asset>();
        DisunityHost.SetRenderPipeline(asset, null);
        Expect.That(RenderPipelineManager.currentPipeline == null && asset.Creations == 0, "nothing is created before a step");
        DisunityHost.Step(0.016f);
        Expect.That(ReferenceEquals(RenderPipelineManager.currentPipeline, asset.Created) && asset.Created != null, "the step makes the asset's pipeline current");
        Expect.Equal(1, _created, "activeRenderPipelineCreated fired once");
        DisunityHost.Step(0.016f);
        DisunityHost.Step(0.016f);
        Expect.That(asset.Creations == 1 && _created == 1 && _disposedEvents == 0, "later steps create nothing new");
        Expect.Equal(0, Pipeline.Renders, "the engine does not render through the pipeline");
        DisunityHost.SetRenderPipeline(null, null);
        DisunityHost.Step(0.016f);
    }

    private static void ChangingTheAssetDisposesTheOldAndCreatesTheNew()
    {
        Reset();
        var first = ScriptableObject.CreateInstance<Asset>();
        var second = ScriptableObject.CreateInstance<Asset>();
        DisunityHost.SetRenderPipeline(first, null);
        DisunityHost.Step(0.016f);
        var old = first.Created!;
        DisunityHost.SetRenderPipeline(second, null);
        DisunityHost.Step(0.016f);
        Expect.That(old.Disposals == 1 && old.disposed && !old.DisposedAsFinalizer, "the old pipeline's Dispose(true) ran once");
        Expect.Equal(1, _disposedEvents, "activeRenderPipelineDisposed fired");
        DisunityHost.Step(0.016f);
        Expect.That(ReferenceEquals(RenderPipelineManager.currentPipeline, second.Created) && second.Creations == 1, "the next step creates the new pipeline");
        Expect.That(_created == 2 && first.Creations == 1 && old.Disposals == 1, "the first asset is not created again");
        DisunityHost.SetQualityLevels(["Low"], 0);
        var quality = ScriptableObject.CreateInstance<Asset>();
        QualitySettings.renderPipeline = quality;
        DisunityHost.Step(0.016f);
        DisunityHost.Step(0.016f);
        Expect.That(ReferenceEquals(RenderPipelineManager.currentPipeline, quality.Created) && second.Created!.Disposals == 1, "a quality level's asset replaces the default's pipeline");
        QualitySettings.renderPipeline = null;
        DisunityHost.SetRenderPipeline(null, null);
        DisunityHost.Step(0.016f);
    }

    private static void ClearingTheAssetDisposesThePipeline()
    {
        Reset();
        var asset = ScriptableObject.CreateInstance<Asset>();
        DisunityHost.SetRenderPipeline(asset, null);
        DisunityHost.Step(0.016f);
        DisunityHost.SetRenderPipeline(null, null);
        DisunityHost.Step(0.016f);
        Expect.That(RenderPipelineManager.currentPipeline == null && asset.Created!.Disposals == 1, "clearing the asset disposes the pipeline and leaves none current");
        Expect.Equal(1, _disposedEvents, "activeRenderPipelineDisposed fired");
        DisunityHost.Step(0.016f);
        Expect.That(RenderPipelineManager.currentPipeline == null && _created == 1 && asset.Creations == 1, "and nothing is created without an asset");
    }

    private static void UnloadingTheWorldDisposesThePipeline()
    {
        Reset();
        var asset = ScriptableObject.CreateInstance<Asset>();
        DisunityHost.SetRenderPipeline(asset, null);
        DisunityHost.Step(0.016f);
        DisunityHost.UnloadWorld();
        Expect.That(RenderPipelineManager.currentPipeline == null && asset.Created!.Disposals == 1, "unloading the world disposes the pipeline");
        Expect.Equal(1, _disposedEvents, "activeRenderPipelineDisposed fired");
        DisunityHost.SetRenderPipeline(null, null);
        DisunityHost.LoadWorld();
        DisunityHost.SetRenderPipeline(asset, null);
        DisunityHost.Step(0.016f);
        Expect.That(asset.Creations == 2 && RenderPipelineManager.currentPipeline == asset.Created, "a new world creates the pipeline again");
        DisunityHost.SetRenderPipeline(null, null);
        DisunityHost.Step(0.016f);
    }

    private static void AFailingCreationIsReportedOnce()
    {
        Reset();
        var failing = ScriptableObject.CreateInstance<FailingAsset>();
        var before = DisunityHost.ScriptExceptionCount;
        DisunityHost.SetRenderPipeline(failing, null);
        for (var step = 0; step < 4; step++) DisunityHost.Step(0.016f);
        Expect.That(failing.Attempts == 1 && DisunityHost.ScriptExceptionCount == before + 1, "a throwing CreatePipeline is tried and reported once");
        Expect.That(RenderPipelineManager.currentPipeline == null && _created == 0, "no pipeline is current and none was announced");
        var good = ScriptableObject.CreateInstance<Asset>();
        DisunityHost.SetRenderPipeline(good, null);
        DisunityHost.Step(0.016f);
        DisunityHost.Step(0.016f);
        Expect.That(ReferenceEquals(RenderPipelineManager.currentPipeline, good.Created) && good.Created != null, "a later working asset still creates its pipeline");
        Expect.Equal(0, _disposedEvents, "a failed asset has no pipeline to dispose");
        DisunityHost.SetRenderPipeline(null, null);
        DisunityHost.Step(0.016f);
    }

    private static void DisposeRunsOnceAndTheArrayFormRendersThroughTheListForm()
    {
        var pipeline = new Pipeline();
        pipeline.Dispose();
        pipeline.Dispose();
        Expect.That(pipeline.disposed && pipeline.Disposals == 1, "Dispose() runs Dispose(bool) once however often it is called");
        var listPipeline = new ListPipeline();
        listPipeline.RenderArray([null!, null!, null!]);
        Expect.That(listPipeline.ListCalls == 1 && listPipeline.ListLength == 3, "the array form passes its cameras to the list form");
    }
}
