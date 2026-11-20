using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The casters of a rendered frame are gathered once however many culls ask for them, and a frame or a change to the
/// world's renderers makes the next shadow query gather again.</summary>
internal static class ShadowCasterCacheTests
{
    private sealed class CullTwicePipeline : RenderPipeline
    {
        protected override void Render(ScriptableRenderContext context, List<Camera> cameras)
        {
            foreach (var camera in cameras)
            {
                for (var pass = 0; pass < 2; pass++)
                {
                    if (!camera.TryGetCullingParameters(out var parameters)) continue;
                    context.Cull(ref parameters).GetShadowCasterBounds(0, out _);
                }
            }
        }
    }

    private sealed class CullTwiceAsset : RenderPipelineAsset
    {
        protected override RenderPipeline CreatePipeline() => new CullTwicePipeline();
    }

    public static void Run()
    {
        InFreshWorld(TwoCullsInOneFrameGatherOnce);
        Console.WriteLine("PASS: two culls in one frame, whatever their culling mask, share one gather of the shadow casters");
        InFreshWorld(ARendererAddedBetweenFramesIsSeen);
        Console.WriteLine("PASS: a renderer added between frames is a caster of the next frame's culls");
        InFreshWorld(ARendererAddedInsideTheFrameIsSeen);
        Console.WriteLine("PASS: a renderer added, and one destroyed, since the last gather changes the next gather");
        InFreshWorld(EachRenderedFrameGathersOnceAcrossItsCulls);
        Console.WriteLine("PASS: the end of a rendered frame drops the gathered casters");
    }

    private static void InFreshWorld(Action test)
    {
        DisunityHost.LoadWorld();
        try
        {
            test();
            default(ScriptableRenderContext).Submit();
        }
        finally
        {
            GraphicsSettings.defaultRenderPipeline = null;
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.fieldOfView = 60f;
        camera.nearClipPlane = 0.3f;
        camera.farClipPlane = 100f;
        camera.cullingMask = -1;
        return camera;
    }

    private static void NewSun()
    {
        var owner = DisunityHost.CreateGameObject();
        owner.transform.rotation = Quaternion.Euler(50f, -30f, 0f);
        var light = owner.AddComponent<Light>();
        light.type = LightType.Directional;
        light.shadows = LightShadows.Soft;
    }

    private static GameObject NewCube(Vector3 position)
    {
        var cube = GameObject.CreatePrimitive(PrimitiveType.Cube);
        cube.transform.position = position;
        return cube;
    }

    private static Bounds CasterBounds(Camera camera, uint mask)
    {
        Expect.That(camera.TryGetCullingParameters(out var parameters), "the camera can be culled");
        parameters.cullingMask = mask;
        var context = default(ScriptableRenderContext);
        var results = context.Cull(ref parameters);
        Expect.That(results.GetShadowCasterBounds(0, out var bounds), "the sun has casters");
        return bounds;
    }

    private static void TwoCullsInOneFrameGatherOnce()
    {
        var camera = NewCamera();
        NewSun();
        NewCube(new Vector3(0f, 0f, 10f));
        var before = ShadowCasterCache.Gathers;
        var first = CasterBounds(camera, uint.MaxValue);
        var second = CasterBounds(camera, 1u);
        Expect.Equal(1, ShadowCasterCache.Gathers - before, "gathers for two culls of one frame");
        Expect.That(first == second, $"both culls see the same casters: {first} and {second}");
    }

    private static void ARendererAddedBetweenFramesIsSeen()
    {
        var camera = NewCamera();
        NewSun();
        NewCube(new Vector3(0f, 0f, 10f));
        var near = CasterBounds(camera, uint.MaxValue);
        DisunityHost.Step(0.016f);
        NewCube(new Vector3(8f, 0f, 30f));
        DisunityHost.Step(0.016f);
        var both = CasterBounds(camera, uint.MaxValue);
        Expect.That(both.max.z > near.max.z + 10f && both.max.x > near.max.x + 5f, $"the cube added between frames is a caster: {near} then {both}");
    }

    private static void ARendererAddedInsideTheFrameIsSeen()
    {
        var camera = NewCamera();
        NewSun();
        var cube = NewCube(new Vector3(0f, 0f, 10f));
        var near = CasterBounds(camera, uint.MaxValue);
        NewCube(new Vector3(8f, 0f, 30f));
        var both = CasterBounds(camera, uint.MaxValue);
        Expect.That(both.max.z > near.max.z + 10f, $"the cube added after the first gather is a caster: {near} then {both}");
        UnityEngine.Object.DestroyImmediate(cube);
        var far = CasterBounds(camera, uint.MaxValue);
        Expect.That(far.min.z > near.min.z + 10f, $"the destroyed cube no longer is: {both} then {far}");
    }

    private static void EachRenderedFrameGathersOnceAcrossItsCulls()
    {
        NewCamera();
        NewSun();
        NewCube(new Vector3(0f, 0f, 10f));
        GraphicsSettings.defaultRenderPipeline = new CullTwiceAsset();
        DisunityHost.Step(0.016f);
        var before = ShadowCasterCache.Gathers;
        DisunityHost.RenderFrame();
        DisunityHost.RenderFrame();
        Expect.Equal(2, ShadowCasterCache.Gathers - before, "gathers for two renders of one world frame, each with two culls");
    }
}
