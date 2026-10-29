using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;
using EngineObject = UnityEngine.Object;

/// <summary>A player plan's perspective camera builds with every serialized setting.</summary>
internal static class PlayerCameraPlanTests
{
    private const string Header = """{"scene":"level0","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[""";
    private const string Object = """{"name":"Rig","tag":"MainCamera","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[""";

    internal static string CameraJson(bool orthographic) => $$"""
        {"type":"Camera","enabled":true,"clear":"Skybox","background":[0.25,0.5,0.75,1],"orthographic_size":5,"near":0.5,"far":500,"depth":2,
        "orthographic":{{(orthographic ? "true" : "false")}},"field_of_view":50,"culling_mask":4294967291,"rect":[0.125,0.25,0.5,0.75],"target_display":1,
        "allow_hdr":false,"allow_msaa":true,"allow_dynamic_resolution":true,"use_occlusion_culling":false}
        """;

    internal static string Plan(string components) => $"{Header}{Object}{components}]}}]}}";

    internal static LoadedScene Load(string components) =>
        DisunityHost.LoadScene(Plan(components), new SceneLoadOptions(typeof(PlayerCameraPlanTests).Assembly, _ => []));

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            PerspectiveCameraBuilds();
            OrthographicCameraKeepsItsLens();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: a player plan builds a perspective camera with its field of view, mask, rect, display and flags");
    }

    private static void PerspectiveCameraBuilds()
    {
        using var loaded = Load(CameraJson(orthographic: false));
        var camera = loaded.Objects[0].GetComponent<Camera>()!;
        Ensure(Camera.main == camera, "the MainCamera-tagged camera is Camera.main");
        Ensure(!camera.orthographic, "the camera is perspective");
        Ensure(camera.fieldOfView == 50f, $"field of view {camera.fieldOfView}");
        Ensure(camera.cullingMask == unchecked((int)0xffff_fffb), $"culling mask {camera.cullingMask}");
        Ensure(camera.rect == new Rect(0.125f, 0.25f, 0.5f, 0.75f), $"rect {camera.rect}");
        Ensure(camera.clearFlags == CameraClearFlags.Skybox, $"clear flags {camera.clearFlags}");
        Ensure(camera.backgroundColor == new Color(0.25f, 0.5f, 0.75f, 1f), "background color");
        Ensure(camera.nearClipPlane == 0.5f && camera.farClipPlane == 500f && camera.depth == 2f, "clip planes and depth");
        Ensure(camera.targetDisplay == 1, $"target display {camera.targetDisplay}");
        Ensure(!camera.allowHDR && camera.allowMSAA && camera.allowDynamicResolution && !camera.useOcclusionCulling, "output flags and occlusion culling");
    }

    private static void OrthographicCameraKeepsItsLens()
    {
        using var loaded = Load(CameraJson(orthographic: true));
        var camera = loaded.Objects[0].GetComponent<Camera>()!;
        Ensure(camera.orthographic && camera.orthographicSize == 5f, "an orthographic plan camera stays orthographic with its size");
    }
}
