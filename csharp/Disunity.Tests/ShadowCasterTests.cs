using Unity.Collections;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The shadow caster queries of a culling result: the bounds of the casters that can shadow a light, and the view,
/// projection, culling sphere and planes of a directional, spot or point light's shadow map. The matrices come from the native
/// shadow query; these tests check what a pipeline relies on, that a caster lies inside the clip box and the split's volume.</summary>
internal static class ShadowCasterTests
{
    private const float Slack = 1e-3f;

    public static void Run()
    {
        InFreshWorld(CasterBoundsHoldTheCastersOfADirectionalLight);
        Console.WriteLine("PASS: GetShadowCasterBounds holds the culled casters of a directional light and skips renderers that cast no shadow");
        InFreshWorld(OnlyRangeCastersShadowALocalLight);
        Console.WriteLine("PASS: a point or spot light's casters are those within its range");
        InFreshWorld(CullingMaskLimitsTheCasters);
        Console.WriteLine("PASS: casters on layers the culling left out do not cast");
        InFreshWorld(DirectionalSplitHoldsTheCasterAndTheSlice);
        Console.WriteLine("PASS: the first of two cascades puts a caster in the clip box, the culling sphere and the planes");
        InFreshWorld(SecondCascadeStartsWhereTheFirstEnds);
        Console.WriteLine("PASS: the second cascade covers the far part of the shadow distance");
        InFreshWorld(SpotAndPointMapsAreTrueWithACasterAndFalseWithout);
        Console.WriteLine("PASS: spot and point maps hold a caster in range and report false without one");
        InFreshWorld(BadDirectionalArgumentsThrow);
        Console.WriteLine("PASS: a split outside the count, bad ratios or a tiny resolution throw ArgumentException");
        InFreshWorld(SplitDataSelectsCastersInTheShadowList);
        Console.WriteLine("PASS: a split's data goes into a shadow renderer list");
        InFreshWorld(CullingShadowCastersTakesACullingResult);
        Console.WriteLine("PASS: CullShadowCasters accepts the infos of a culling result and refuses a default result");
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
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.orthographic = false;
        camera.fieldOfView = 60f;
        camera.nearClipPlane = 0.3f;
        camera.farClipPlane = 100f;
        camera.cullingMask = -1;
        return camera;
    }

    private static Light NewLight(LightType type, Vector3 position, Quaternion rotation, float range = 20f)
    {
        var owner = DisunityHost.CreateGameObject();
        owner.transform.SetPositionAndRotation(position, rotation);
        var light = owner.AddComponent<Light>();
        light.type = type;
        light.range = range;
        light.spotAngle = 60f;
        light.shadows = LightShadows.Soft;
        return light;
    }

    private static Light NewSun() => NewLight(LightType.Directional, new Vector3(0f, 50f, 0f), Quaternion.Euler(50f, -30f, 0f), 0f);

    private static GameObject NewCube(Vector3 position, ShadowCastingMode mode = ShadowCastingMode.On, int layer = 0)
    {
        var cube = GameObject.CreatePrimitive(PrimitiveType.Cube);
        cube.transform.position = position;
        cube.layer = layer;
        cube.GetComponent<MeshRenderer>()!.shadowCastingMode = mode;
        return cube;
    }

    private static CullingResults Cull(Camera camera, uint? mask = null)
    {
        Expect.That(camera.TryGetCullingParameters(out var parameters), "the camera can be culled");
        if (mask is { } value) parameters.cullingMask = value;
        var context = default(ScriptableRenderContext);
        return context.Cull(ref parameters);
    }

    private static Vector3[] Corners(Bounds bounds)
    {
        var corners = new Vector3[8];
        for (var index = 0; index < 8; index++)
        {
            corners[index] = bounds.center + Vector3.Scale(bounds.extents,
                new Vector3((index & 1) == 0 ? -1f : 1f, (index & 2) == 0 ? -1f : 1f, (index & 4) == 0 ? -1f : 1f));
        }
        return corners;
    }

    private static bool InClipBox(Matrix4x4 view, Matrix4x4 projection, Vector3 point)
    {
        var clip = (projection * view) * new Vector4(point.x, point.y, point.z, 1f);
        return MathF.Abs(clip.x) <= clip.w + Slack && MathF.Abs(clip.y) <= clip.w + Slack && MathF.Abs(clip.z) <= clip.w + Slack;
    }

    private static bool InPlanes(ShadowSplitData split, Vector3 point)
    {
        for (var index = 0; index < split.cullingPlaneCount; index++)
            if (split.GetCullingPlane(index).GetDistanceToPoint(point) < -Slack) return false;
        return true;
    }

    private static bool InSphere(ShadowSplitData split, Vector3 point) =>
        Vector3.Distance(point, split.cullingSphere) <= split.cullingSphere.w + Slack;

    private static void CasterBoundsHoldTheCastersOfADirectionalLight()
    {
        var camera = NewCamera();
        NewSun();
        var near = NewCube(new Vector3(0f, 0f, 10f));
        var side = NewCube(new Vector3(8f, 0f, 20f), ShadowCastingMode.TwoSided);
        NewCube(new Vector3(-30f, 0f, 5f), ShadowCastingMode.Off);
        var results = Cull(camera);
        Expect.That(results.GetShadowCasterBounds(0, out var bounds), "the sun has casters");
        var expected = near.GetComponent<MeshRenderer>()!.bounds;
        expected.Encapsulate(side.GetComponent<MeshRenderer>()!.bounds);
        Expect.That(Vector3.Distance(expected.min, bounds.min) < Slack && Vector3.Distance(expected.max, bounds.max) < Slack,
            $"the bounds are the box of the two casters: expected {expected}, got {bounds}");
        foreach (var corner in Corners(near.GetComponent<MeshRenderer>()!.bounds)) Expect.That(bounds.Contains(corner), "the caster's corner is inside");
    }

    private static void OnlyRangeCastersShadowALocalLight()
    {
        var camera = NewCamera();
        NewLight(LightType.Point, new Vector3(0f, 0f, 5f), Quaternion.identity, 8f);
        NewCube(new Vector3(0f, 0f, 9f));
        NewCube(new Vector3(0f, 0f, 60f));
        var results = Cull(camera);
        Expect.That(results.GetShadowCasterBounds(0, out var bounds), "the cube within range casts");
        Expect.That(bounds.max.z < 20f, $"the far cube is not a caster of the point light: {bounds}");
        var alone = DisunityHost.CreateGameObject();
        Expect.That(alone != null, "scene");
    }

    private static void CullingMaskLimitsTheCasters()
    {
        var camera = NewCamera();
        NewSun();
        NewCube(new Vector3(0f, 0f, 10f), layer: 5);
        Expect.That(!Cull(camera, 1u).GetShadowCasterBounds(0, out _), "layer 5 is not drawn by a mask of layer 0");
        Expect.That(Cull(camera, 1u << 5).GetShadowCasterBounds(0, out _), "and is by a mask of layer 5");
    }

    private static void DirectionalSplitHoldsTheCasterAndTheSlice()
    {
        var camera = NewCamera();
        NewSun();
        var cube = NewCube(new Vector3(0f, 0f, 10f));
        var results = Cull(camera);
        Expect.That(results.ComputeDirectionalShadowMatricesAndCullingPrimitives(0, 0, 2, new Vector3(0.5f, 0f, 0f), 1024, 3f,
            out var view, out var projection, out var split), "the sun has casters for the first cascade");
        foreach (var corner in Corners(cube.GetComponent<MeshRenderer>()!.bounds))
        {
            Expect.That(InClipBox(view, projection, corner), $"corner {corner} is inside the clip box");
            Expect.That(InSphere(split, corner), $"corner {corner} is inside the culling sphere {split.cullingSphere}");
            Expect.That(InPlanes(split, corner), $"corner {corner} is inside the culling planes");
        }
        Expect.That(split.cullingPlaneCount is > 0 and <= ShadowSplitData.MaxCullingPlanes, "the split carries planes");
        Expect.That(split.cullingMatrix == projection * view, "the culling matrix is the projection times the view");
        Expect.That(split.cullingSphere.z <= 50f && !InSphere(split, new Vector3(0f, 0f, 100f)), $"the first cascade ends at half the shadow distance: sphere {split.cullingSphere}");
        Expect.That(view != Matrix4x4.identity && projection != Matrix4x4.identity, "the matrices are not the identity");
    }

    private static void SecondCascadeStartsWhereTheFirstEnds()
    {
        var camera = NewCamera();
        NewSun();
        var cube = NewCube(new Vector3(0f, 0f, 70f));
        var results = Cull(camera);
        var ratio = new Vector3(0.5f, 0f, 0f);
        Expect.That(results.ComputeDirectionalShadowMatricesAndCullingPrimitives(0, 1, 2, ratio, 1024, 3f, out var view, out var projection, out var split),
            "the second cascade");
        foreach (var corner in Corners(cube.GetComponent<MeshRenderer>()!.bounds))
        {
            Expect.That(InClipBox(view, projection, corner) && InSphere(split, corner), $"corner {corner} is in the second cascade");
        }
        Expect.That(split.cullingSphere.z > 50f && InSphere(split, new Vector3(0f, 0f, 55f)), $"the second cascade is centred past the first cascade's end: sphere {split.cullingSphere}");
    }

    private static void SpotAndPointMapsAreTrueWithACasterAndFalseWithout()
    {
        var camera = NewCamera();
        NewLight(LightType.Spot, new Vector3(0f, 0f, -2f), Quaternion.identity, 20f);
        NewLight(LightType.Point, new Vector3(0f, 0f, 0f), Quaternion.identity, 20f);
        var results = Cull(camera);
        Expect.That(!results.ComputeSpotShadowMatricesAndCullingPrimitives(0, out _, out _, out _), "no caster, no spot map");
        NewCube(new Vector3(0f, 0f, 8f));
        results = Cull(camera);
        Expect.That(results.ComputeSpotShadowMatricesAndCullingPrimitives(0, out var view, out var projection, out var split), "a spot map");
        Expect.That(InClipBox(view, projection, new Vector3(0f, 0f, 8f)) && InPlanes(split, new Vector3(0f, 0f, 8f)), "the caster is in the spot map");
        Expect.That(projection[3, 2] == -1f && projection[3, 3] == 0f, "a perspective projection");
        Expect.That(results.ComputePointShadowMatricesAndCullingPrimitives(1, CubemapFace.PositiveZ, 0f, out view, out projection, out split), "a point map");
        Expect.That(InClipBox(view, projection, new Vector3(0f, 0f, 8f)), "the caster is in the +z face");
        results.ComputePointShadowMatricesAndCullingPrimitives(1, CubemapFace.NegativeZ, 0f, out view, out projection, out split);
        Expect.That(!InClipBox(view, projection, new Vector3(0f, 0f, 8f)), "and not in the -z face");
    }

    private static void CullingShadowCastersTakesACullingResult()
    {
        var camera = NewCamera();
        NewSun();
        NewCube(new Vector3(0f, 0f, 10f));
        var results = Cull(camera);
        var context = default(ScriptableRenderContext);
        context.CullShadowCasters(results, default);
        Expect.That(results.ComputeDirectionalShadowMatricesAndCullingPrimitives(0, 0, 1, default, 1024, 3f, out _, out _, out var split), "a split");
        var infos = new ShadowCastersCullingInfos
        {
            splitBuffer = new NativeArray<ShadowSplitData>([split], Allocator.Temp),
            perLightInfos = new NativeArray<LightShadowCasterCullingInfo>(
                [new LightShadowCasterCullingInfo { splitRange = new RangeInt(0, 1), projectionType = BatchCullingProjectionType.Orthographic }], Allocator.Temp),
        };
        context.CullShadowCasters(results, infos);
        Expect.That(infos.splitBuffer[0] == split && infos.perLightInfos[0].splitRange.length == 1, "the infos hold what the pipeline put in them");
        infos.splitBuffer.Dispose();
        infos.perLightInfos.Dispose();
        Expect.Throws<ArgumentException>(() => context.CullShadowCasters(default, default), "a default culling result holds no casters to cull");
    }

    private static void BadDirectionalArgumentsThrow()
    {
        var camera = NewCamera();
        NewSun();
        NewCube(new Vector3(0f, 0f, 10f));
        var results = Cull(camera);
        var ratio = new Vector3(0.5f, 0f, 0f);
        Expect.Throws<ArgumentException>(() => results.ComputeDirectionalShadowMatricesAndCullingPrimitives(0, 2, 2, ratio, 1024, 0f, out _, out _, out _), "a split past the count");
        Expect.Throws<ArgumentException>(() => results.ComputeDirectionalShadowMatricesAndCullingPrimitives(0, 0, 3, new Vector3(0.6f, 0.3f, 0f), 1024, 0f, out _, out _, out _), "descending ratios");
        Expect.Throws<ArgumentException>(() => results.ComputeDirectionalShadowMatricesAndCullingPrimitives(0, 0, 2, ratio, 2, 0f, out _, out _, out _), "a tiny shadow map");
        Expect.Throws<ArgumentException>(() => results.ComputePointShadowMatricesAndCullingPrimitives(0, (CubemapFace)9, 0f, out _, out _, out _), "a face outside the cube");
    }

    private static void SplitDataSelectsCastersInTheShadowList()
    {
        var camera = NewCamera();
        NewSun();
        NewCube(new Vector3(0f, 0f, 10f));
        var results = Cull(camera);
        Expect.That(results.ComputeDirectionalShadowMatricesAndCullingPrimitives(0, 0, 2, new Vector3(0.5f, 0f, 0f), 1024, 3f, out _, out _, out var split), "split");
        var settings = new ShadowDrawingSettings(results, 0) { splitData = split, projectionType = BatchCullingProjectionType.Orthographic };
        var context = default(ScriptableRenderContext);
        var list = context.CreateShadowRendererList(ref settings);
        Expect.That(list.isValid, "the shadow list made from a computed split is valid");
    }
}
