using System.Globalization;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>Camera.TryGetCullingParameters fills the parameters from the camera, and ScriptableRenderContext.Cull records a
/// culling result in the world's offscreen list and returns a handle to it.</summary>
internal static class CullingTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ParametersComeFromTheCamera();
            Console.WriteLine("PASS: Camera.TryGetCullingParameters fills the mask, planes, matrix, origin, lod and shadow distance from the camera");
            ParametersRefuseWhatCannotBeCulled();
            Console.WriteLine("PASS: TryGetCullingParameters returns false for a camera without pixels and throws for a destroyed one");
            PlanesAndLayersAreChecked();
            Console.WriteLine("PASS: ScriptableCullingParameters checks plane indexes, plane counts and layers, and compares by value");
            CullRecordsAtSubmit();
            Console.WriteLine("PASS: Cull returns a distinct handle and records the culling result in call order when submitted");
            CullNeedsParametersFromACamera();
            Console.WriteLine("PASS: Cull refuses parameters that did not come from a camera");
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
        return camera;
    }

    private static string Show(float value) => value.ToString("R", CultureInfo.InvariantCulture);

    private static void ParametersComeFromTheCamera()
    {
        var camera = NewCamera();
        camera.transform.position = new Vector3(1f, 2f, 3f);
        camera.cullingMask = 0x105;
        Expect.That(camera.TryGetCullingParameters(out var p), "a camera with pixels can be culled");
        Expect.Equal(6, p.cullingPlaneCount, "a view volume has six planes");
        var expected = GeometryUtility.CalculateFrustumPlanes(camera);
        for (var i = 0; i < 6; i++)
        {
            var plane = p.GetCullingPlane(i);
            Expect.That((plane.normal - expected[i].normal).magnitude < 1e-4f, $"plane {i} normal");
            Expect.Near(expected[i].distance, plane.distance, $"plane {i} distance");
        }
        Expect.Equal(0x105u, p.cullingMask, "the culling mask");
        Expect.That(!p.isOrthographic, "a perspective camera");
        Expect.Equal(new Vector3(1f, 2f, 3f), p.origin, "the origin is the camera position");
        var matrix = camera.projectionMatrix * camera.worldToCameraMatrix;
        for (var i = 0; i < 16; i++) Expect.Near(matrix[i], p.cullingMatrix[i], $"culling matrix element {i}", 1e-3f);
        Expect.Near(60f, p.lodParameters.fieldOfView, "lod field of view");
        Expect.Equal(new Vector3(1f, 2f, 3f), p.lodParameters.cameraPosition, "lod position");
        Expect.Equal(camera.pixelHeight, p.lodParameters.cameraPixelHeight, "lod pixel height");
        Expect.That(!p.lodParameters.isOrthographic, "lod projection");
        Expect.Equal(CullingOptions.None, p.cullingOptions, "the engine culls with no options");
        Expect.Equal(32, p.layerCount, "one distance per layer");
        var quality = QualitySettings.shadowDistance;
        QualitySettings.shadowDistance = 20f;
        camera.TryGetCullingParameters(out var near);
        Expect.Near(20f, near.shadowDistance, "the shadow distance comes from the quality settings");
        camera.farClipPlane = 10f;
        camera.TryGetCullingParameters(out var clipped);
        Expect.Near(10f, clipped.shadowDistance, "and stops at the far plane");
        QualitySettings.shadowDistance = quality;
        camera.farClipPlane = 100f;
        camera.TryGetCullingParameters(true, out var stereo);
        Expect.Equal(p.cullingMask, stereo.cullingMask, "the stereo-aware form fills the same parameters, the engine has no stereo");
        camera.orthographic = true;
        camera.orthographicSize = 4f;
        camera.TryGetCullingParameters(out var ortho);
        Expect.That(ortho.isOrthographic && ortho.lodParameters.isOrthographic, "an orthographic camera");
        Expect.Near(4f, ortho.lodParameters.orthoSize, "lod orthographic size");
        camera.orthographic = false;
        var distances = new float[32];
        distances[5] = 7f;
        camera.layerCullDistances = distances;
        camera.TryGetCullingParameters(out var layered);
        Expect.Near(7f, layered.GetLayerCullingDistance(5), "a layer cull distance is copied");
        Expect.Near(0f, layered.GetLayerCullingDistance(4), "other layers have none");
    }

    private static void ParametersRefuseWhatCannotBeCulled()
    {
        var camera = NewCamera();
        camera.rect = new Rect(0, 0, 0, 0);
        Expect.That(!camera.TryGetCullingParameters(out var none), "a camera with no pixels cannot be culled");
        Expect.Equal(0, none.cullingPlaneCount, "and fills nothing");
        var destroyed = NewCamera();
        UnityEngine.Object.DestroyImmediate(destroyed);
        Expect.Throws<InvalidOperationException>(() => destroyed.TryGetCullingParameters(out _), "a destroyed camera");
    }

    private static void PlanesAndLayersAreChecked()
    {
        var camera = NewCamera();
        camera.TryGetCullingParameters(out var p);
        var plane = new Plane(Vector3.up, 3f);
        Expect.Throws<IndexOutOfRangeException>(() => p.GetCullingPlane(6), "plane 6 of 6");
        Expect.Throws<IndexOutOfRangeException>(() => p.SetCullingPlane(-1, plane), "plane -1");
        Expect.Throws<ArgumentException>(() => p.cullingPlaneCount = 11, "11 planes");
        Expect.Throws<ArgumentException>(() => p.cullingPlaneCount = -1, "negative planes");
        Expect.Throws<IndexOutOfRangeException>(() => p.GetLayerCullingDistance(32), "layer 32");
        Expect.Throws<IndexOutOfRangeException>(() => p.SetLayerCullingDistance(-1, 1f), "layer -1");
        p.cullingPlaneCount = 7;
        p.SetCullingPlane(6, plane);
        Expect.Equal(Vector3.up, p.GetCullingPlane(6).normal, "a plane reads back");
        var copy = p;
        Expect.That(copy == p && copy.GetHashCode() == p.GetHashCode() && copy.Equals((object)p), "equal parameters");
        copy.SetCullingPlane(6, new Plane(Vector3.right, 3f));
        Expect.That(copy != p, "a plane takes part in equality");
        copy = p;
        copy.cullingMask = 3;
        Expect.That(copy != p, "the mask takes part in equality");
        copy = p;
        copy.SetLayerCullingDistance(2, 9f);
        Expect.That(copy != p, "a layer distance takes part in equality");
        copy = p;
        copy.maximumVisibleLights = 8;
        Expect.Equal(8, copy.maximumVisibleLights, "the light limit is stored");
        Expect.That(copy != p, "the light limit takes part in equality");
    }

    private static void CullRecordsAtSubmit()
    {
        var first = NewCamera();
        var second = NewCamera();
        first.cullingMask = 0x105;
        first.TryGetCullingParameters(out var one);
        second.TryGetCullingParameters(out var two);
        CommandWireReader.Take();
        var context = default(ScriptableRenderContext);
        var a = context.Cull(ref one);
        var b = context.Cull(ref one);
        var c = context.Cull(ref two);
        Expect.That(a != b && b != c && a != c && a.Equals(a), "each Cull returns its own handle");
        Expect.That(default(CullingResults) != a, "a handle is not the empty result");
        Expect.Equal(0, a.visibleLights.Length, "the engine has no light culling, so no lights are visible");
        Expect.Equal(0, a.visibleReflectionProbes.Length, "and no probes");
        Expect.Equal(0, CommandWireReader.Take().Length, "nothing reaches the world before Submit");
        context.Submit();
        var taken = CommandWireReader.Take();
        Expect.Equal(3, taken.Length, "three culling results in call order");
        Expect.That(taken.All(t => t.StartsWith("Cull id=", StringComparison.Ordinal)), $"culling results: {string.Join("|", taken)}");
        var expected = GeometryUtility.CalculateFrustumPlanes(first);
        var planes = string.Join(",", expected.Select(p => $"({Show(p.normal.x)} {Show(p.normal.y)} {Show(p.normal.z)} {Show(p.distance)})"));
        Expect.That(taken[0].Contains("mask=261 ") && taken[0].EndsWith(" options=0", StringComparison.Ordinal), $"mask and options: {taken[0]}");
        Expect.That(taken[0].Contains($"planes=[{planes}]", StringComparison.Ordinal), $"the planes of the camera: {taken[0]}");
        var camera = (string s) => s.Split(' ')[2];
        Expect.That(camera(taken[0]) == camera(taken[1]) && camera(taken[0]) != camera(taken[2]), "each record names its camera");
        var ids = taken.Select(t => t.Split(' ')[1]).ToArray();
        Expect.Equal(3, ids.Distinct().Count(), "the ids are distinct");
    }

    private static void CullNeedsParametersFromACamera()
    {
        var context = default(ScriptableRenderContext);
        var empty = default(ScriptableCullingParameters);
        Expect.Throws<InvalidOperationException>(() => context.Cull(ref empty), "parameters that did not come from a camera");
    }
}
