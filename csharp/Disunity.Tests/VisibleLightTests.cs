using Disunity.Hosting;
using Unity.Collections;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>ScriptableRenderContext.Cull collects the visible lights at the call, and the CullingResults it returns reports
/// them, serves their index maps and answers the shadow queries for a pipeline that finds no shadow casters.</summary>
internal static class VisibleLightTests
{
    private const float Tolerance = 1e-4f;

    public static void Run()
    {
        InFreshWorld(CullKeepsTheVisibleLights);
        Console.WriteLine("PASS: Cull reports the directional light first and the point and spot lights whose sphere meets the frustum");
        InFreshWorld(GroupsKeepCreationOrder);
        Console.WriteLine("PASS: directional lights come first and each group keeps creation order");
        InFreshWorld(FarPlaneFlag);
        Console.WriteLine("PASS: a light whose sphere crosses the far plane reports IntersectsFarPlane");
        InFreshWorld(FinalColorFollowsTheColorSpace);
        Console.WriteLine("PASS: finalColor is color times intensity, linear when the project is linear and lights use linear intensity");
        InFreshWorld(ResultsKeepTheirOwnLights);
        Console.WriteLine("PASS: each culling result keeps the lights found when Cull ran");
        InFreshWorld(IndexMaps);
        Console.WriteLine("PASS: the light and reflection probe index maps are identity maps that SetLightIndexMap validates");
        InFreshWorld(ShadowQueries);
        Console.WriteLine("PASS: shadow queries report no casters for a visible light and refuse an index outside the visible lights");
    }

    /// <summary>Each case starts from a world with no lights, so the lights of an earlier case are not seen. The culling records a
    /// case queued are submitted while its world is still loaded, so a later test does not send records for a world that is gone.</summary>
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

    private static Light NewLight(LightType type, Vector3 position, float range = 5f)
    {
        var owner = DisunityHost.CreateGameObject();
        owner.transform.position = position;
        var light = owner.AddComponent<Light>();
        light.type = type;
        light.range = range;
        light.color = Color.white;
        light.intensity = 1f;
        return light;
    }

    private static CullingResults Cull(Camera camera, uint? mask = null)
    {
        Expect.That(camera.TryGetCullingParameters(out var parameters), "the camera can be culled");
        if (mask is { } value) parameters.cullingMask = value;
        var context = default(ScriptableRenderContext);
        return context.Cull(ref parameters);
    }

    private static void CullKeepsTheVisibleLights()
    {
        var camera = NewCamera();
        var inside = NewLight(LightType.Point, new Vector3(0f, 0f, 10f), 5f);
        inside.color = new Color(0.5f, 0.25f, 1f, 1f);
        inside.intensity = 2f;
        NewLight(LightType.Point, new Vector3(500f, 0f, 10f), 5f);
        var spot = NewLight(LightType.Spot, new Vector3(0f, 0f, -3f), 3.5f);
        spot.spotAngle = 40f;
        spot.transform.rotation = Quaternion.Euler(10f, 20f, 0f);
        var sun = NewLight(LightType.Directional, new Vector3(0f, 50f, 0f), 0f);
        sun.transform.rotation = Quaternion.Euler(50f, -30f, 0f);
        sun.color = new Color(1f, 0.9f, 0.8f, 1f);
        sun.intensity = 1.5f;
        var disabled = NewLight(LightType.Point, new Vector3(0f, 0f, 12f));
        disabled.enabled = false;
        var inactive = NewLight(LightType.Point, new Vector3(0f, 0f, 14f));
        inactive.gameObject.SetActive(false);
        var otherLayer = NewLight(LightType.Point, new Vector3(0f, 0f, 16f));
        otherLayer.gameObject.layer = 5;

        var results = Cull(camera, mask: 1u);
        var lights = results.visibleLights;
        Expect.Equal(4, lights.Length, "the directional light and the three lights in the frustum, whatever their own layer");
        Expect.That(lights[0].light == sun && lights[1].light == inside && lights[2].light == spot && lights[3].light == otherLayer,
            "directional first, then creation order; a light on a layer the camera does not draw still lights what it draws");

        Expect.Equal(LightType.Directional, lights[0].lightType, "directional type");
        Expect.Equal(VisibleLightFlags.None, lights[0].flags, "a directional light has no plane flags");
        Expect.That(NearColor(lights[0].finalColor, new Color(1.5f, 1.35f, 1.2f, 1f)), "directional final color");
        Expect.That(lights[0].localToWorldMatrix == sun.transform.localToWorldMatrix, "directional matrix");
        Expect.That(lights[0].screenRect == new Rect(0f, 0f, 1f, 1f) && lights[0].occlusionMaskChannel == -1, "directional rect and channel");

        Expect.Equal(LightType.Point, lights[1].lightType, "point type");
        Expect.Near(5f, lights[1].range, "point range");
        Expect.That(NearColor(lights[1].finalColor, new Color(1f, 0.5f, 2f, 1f)), "point final color is color times intensity");
        Expect.That(lights[1].localToWorldMatrix == inside.transform.localToWorldMatrix, "point matrix");
        Expect.Equal(VisibleLightFlags.None, lights[1].flags, "a sphere well inside crosses neither plane");
        Expect.That(lights[1].screenRect == new Rect(0f, 0f, 1f, 1f) && lights[1].occlusionMaskChannel == -1, "point rect and channel");

        Expect.Equal(LightType.Spot, lights[2].lightType, "spot type");
        Expect.Near(3.5f, lights[2].range, "spot range");
        Expect.Near(40f, lights[2].spotAngle, "spot angle");
        Expect.That(lights[2].localToWorldMatrix == spot.transform.localToWorldMatrix, "spot matrix");
        Expect.Equal(VisibleLightFlags.IntersectsNearPlane, lights[2].flags, "a sphere that reaches past the near plane");

        Expect.Equal(0, results.visibleOffscreenVertexLights.Length, "no offscreen vertex lights");
        Expect.Equal(0, results.visibleReflectionProbes.Length, "no reflection probes");
        Expect.Equal(0, results.lightIndexCount, "no per-object light lists");
        Expect.Equal(0, results.lightAndReflectionProbeIndexCount, "no per-object light and probe lists");
    }

    private static bool NearColor(Color a, Color b) =>
        MathF.Abs(a.r - b.r) < Tolerance && MathF.Abs(a.g - b.g) < Tolerance && MathF.Abs(a.b - b.b) < Tolerance && MathF.Abs(a.a - b.a) < Tolerance;

    private static void GroupsKeepCreationOrder()
    {
        var camera = NewCamera();
        var point = NewLight(LightType.Point, new Vector3(0f, 0f, 10f));
        var first = NewLight(LightType.Directional, Vector3.zero);
        var spot = NewLight(LightType.Spot, new Vector3(1f, 0f, 20f));
        var second = NewLight(LightType.Directional, Vector3.zero);
        var lights = Cull(camera).visibleLights;
        Expect.Equal(4, lights.Length, "four visible lights");
        Expect.That(lights[0].light == first && lights[1].light == second && lights[2].light == point && lights[3].light == spot,
            "the directional lights, then the rest, each in creation order");
    }

    private static void FarPlaneFlag()
    {
        var camera = NewCamera();
        var edge = NewLight(LightType.Point, new Vector3(0f, 0f, 98f), 5f);
        var lights = Cull(camera).visibleLights;
        Expect.Equal(1, lights.Length, "the light is in the frustum");
        Expect.Equal(VisibleLightFlags.IntersectsFarPlane, lights[0].flags, "the far plane flag");
        Expect.That(lights[0].light == edge, "the light object");
        edge.transform.position = new Vector3(0f, 0f, 108f);
        Expect.Equal(0, Cull(camera).visibleLights.Length, "past the far plane by more than the range");
    }

    private static void FinalColorFollowsTheColorSpace()
    {
        var camera = NewCamera();
        var light = NewLight(LightType.Point, new Vector3(0f, 0f, 10f));
        light.color = new Color(0.5f, 0.25f, 1f, 1f);
        light.intensity = 2f;
        try
        {
            DisunityHost.SetColorSpace(ColorSpace.Gamma);
            GraphicsSettings.lightsUseLinearIntensity = true;
            Expect.That(NearColor(Cull(camera).visibleLights[0].finalColor, new Color(1f, 0.5f, 2f, 1f)), "gamma space uses the color as it is");
            DisunityHost.SetColorSpace(ColorSpace.Linear);
            GraphicsSettings.lightsUseLinearIntensity = false;
            Expect.That(NearColor(Cull(camera).visibleLights[0].finalColor, new Color(1f, 0.5f, 2f, 1f)), "lights without linear intensity use the color as it is");
            GraphicsSettings.lightsUseLinearIntensity = true;
            var expected = light.color.linear * 2f;
            expected.a = 1f;
            Expect.That(NearColor(Cull(camera).visibleLights[0].finalColor, expected), "linear space with linear intensity converts the color first");
            Expect.That(expected.r < 0.5f, "the conversion changed the color");
        }
        finally
        {
            DisunityHost.SetColorSpace(ColorSpace.Gamma);
            GraphicsSettings.lightsUseLinearIntensity = false;
        }
    }

    private static void ResultsKeepTheirOwnLights()
    {
        var camera = NewCamera();
        var onDefault = NewLight(LightType.Point, new Vector3(0f, 0f, 10f));
        var first = Cull(camera, 1u);
        var later = NewLight(LightType.Point, new Vector3(0f, 0f, 11f));
        var second = Cull(camera, 1u);
        Expect.That(first.visibleLights.Length == 1 && first.visibleLights[0].light == onDefault, "the first cull sees the one light");
        Expect.That(second.visibleLights.Length == 2 && second.visibleLights[1].light == later, "the second sees the light made after the first");
        var seen = first.visibleLights[0];
        onDefault.enabled = false;
        onDefault.type = LightType.Spot;
        Expect.That(first.visibleLights[0] == seen && first.visibleLights[0].lightType == LightType.Point,
            "a later change does not alter a result that was already made");
        Expect.Equal(1, Cull(camera, 1u).visibleLights.Length, "a new cull sees the change");
    }

    private static void IndexMaps()
    {
        var camera = NewCamera();
        NewLight(LightType.Directional, Vector3.zero);
        NewLight(LightType.Point, new Vector3(0f, 0f, 10f));
        NewLight(LightType.Spot, new Vector3(0f, 0f, 12f));
        var results = Cull(camera);
        var map = results.GetLightIndexMap(Allocator.Temp);
        try
        {
            Expect.Sequence(new[] { 0, 1, 2 }, map.ToArray(), "an identity map with one entry per visible light");
            map[0] = 2;
            map[2] = 0;
            results.SetLightIndexMap(map);
            var stored = results.GetLightIndexMap(Allocator.Temp);
            Expect.Sequence(new[] { 2, 1, 0 }, stored.ToArray(), "a map that was set is the one read back");
            stored.Dispose();
            map[2] = 5;
            Expect.Sequence(new[] { 2, 1, 0 }, results.GetLightIndexMap(Allocator.Temp).ToArray(), "the result keeps a copy, not the caller's array");
            Expect.Sequence(new[] { 0, 1, 2 }, Cull(camera).GetLightIndexMap(Allocator.Temp).ToArray(), "another result has its own map");
            var wrong = new NativeArray<int>(2, Allocator.Temp);
            Expect.Throws<ArgumentException>(() => results.SetLightIndexMap(wrong), "a map of the wrong length");
            wrong.Dispose();
        }
        finally { map.Dispose(); }
        var probes = results.GetReflectionProbeIndexMap(Allocator.Temp);
        Expect.Equal(0, probes.Length, "no reflection probes, no entries");
        results.SetReflectionProbeIndexMap(probes);
        var extra = new NativeArray<int>(1, Allocator.Temp);
        Expect.Throws<ArgumentException>(() => results.SetReflectionProbeIndexMap(extra), "a probe map with an entry");
        extra.Dispose();
        probes.Dispose();
        var none = default(CullingResults);
        var empty = none.GetLightIndexMap(Allocator.Temp);
        Expect.Equal(0, empty.Length, "a result that holds no lights has an empty map");
        none.SetLightIndexMap(empty);
        empty.Dispose();
    }

    private static void ShadowQueries()
    {
        var camera = NewCamera();
        NewLight(LightType.Directional, Vector3.zero);
        NewLight(LightType.Point, new Vector3(0f, 0f, 10f));
        NewLight(LightType.Spot, new Vector3(0f, 0f, 12f));
        var results = Cull(camera);
        for (var index = 0; index < 3; index++)
        {
            Expect.That(!results.GetShadowCasterBounds(index, out var bounds) && bounds == default, $"no shadow casters for light {index}");
            Expect.That(!results.ComputeSpotShadowMatricesAndCullingPrimitives(index, out var view, out var projection, out var split)
                && Identity(view, projection, split), $"spot shadows for light {index}");
            Expect.That(!results.ComputePointShadowMatricesAndCullingPrimitives(index, CubemapFace.PositiveX, 0f, out view, out projection, out split)
                && Identity(view, projection, split), $"point shadows for light {index}");
            Expect.That(!results.ComputeDirectionalShadowMatricesAndCullingPrimitives(index, 0, 1, Vector3.zero, 512, 0f, out view, out projection, out split)
                && Identity(view, projection, split), $"directional shadows for light {index}");
        }
        foreach (var index in new[] { -1, 3 })
        {
            Expect.Throws<ArgumentOutOfRangeException>(() => results.GetShadowCasterBounds(index, out _), $"bounds of light {index}");
            Expect.Throws<ArgumentOutOfRangeException>(() => results.ComputeSpotShadowMatricesAndCullingPrimitives(index, out _, out _, out _), $"spot light {index}");
            Expect.Throws<ArgumentOutOfRangeException>(
                () => results.ComputePointShadowMatricesAndCullingPrimitives(index, CubemapFace.PositiveX, 0f, out _, out _, out _), $"point light {index}");
            Expect.Throws<ArgumentOutOfRangeException>(
                () => results.ComputeDirectionalShadowMatricesAndCullingPrimitives(index, 0, 1, Vector3.zero, 512, 0f, out _, out _, out _), $"directional light {index}");
        }
        Expect.Throws<ArgumentOutOfRangeException>(() => default(CullingResults).GetShadowCasterBounds(0, out _), "a result that holds no lights");
    }

    private static bool Identity(Matrix4x4 view, Matrix4x4 projection, ShadowSplitData split) =>
        view == Matrix4x4.identity && projection == Matrix4x4.identity && split == default;
}
