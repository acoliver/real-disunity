using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Camera.main, allCameras, CopyFrom, the stored settings and the members the engine cannot run yet.</summary>
internal static class CameraSceneTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            MainCameraRules();
            Console.WriteLine("PASS: Camera.main tagging rules, current and allCameras");
            CopyFromCopiesEverySetting();
            Console.WriteLine("PASS: Camera.CopyFrom copies native and stored settings");
            StoredSettings();
            Console.WriteLine("PASS: Camera stored settings (rendering path, sort mode, layer cull distances, stereo defaults)");
            UnsupportedMembers();
            Console.WriteLine("PASS: Camera members that need a renderer throw NotSupportedException naming the member");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera(string tag = "Untagged", bool active = true)
    {
        var owner = DisunityHost.CreateGameObject();
        owner.tag = tag;
        var camera = owner.AddComponent<Camera>();
        if (!active) owner.SetActive(false);
        return camera;
    }

    private static void MainCameraRules()
    {
        Ensure(Camera.main is null && Camera.allCamerasCount == 0 && Camera.allCameras.Length == 0, "no cameras yet");
        var plain = NewCamera();
        Ensure(Camera.main is null, "an untagged camera is not the main camera");
        var inactive = NewCamera("MainCamera", active: false);
        Ensure(Camera.main is null, "a camera on an inactive object is not the main camera");
        var disabled = NewCamera("MainCamera");
        disabled.enabled = false;
        Ensure(Camera.main is null, "a disabled camera is not the main camera");
        var first = NewCamera("MainCamera");
        var second = NewCamera("MainCamera");
        Ensure(Camera.main == first && Camera.main != second, "the first enabled MainCamera wins");
        first.enabled = false;
        Ensure(Camera.main == second, "disabling the first passes the role on");
        first.enabled = true;
        second.gameObject.tag = "Untagged";
        Ensure(Camera.main == first, "retagging changes the main camera");
        Ensure(Camera.current is null, "no camera is current outside rendering");
        Ensure(Camera.allCamerasCount == 3 && Camera.allCameras.Length == 3, "enabled cameras on active objects: plain, first and second");
        Ensure(Camera.allCameras.Contains(plain) && Camera.allCameras.Contains(second) && !Camera.allCameras.Contains(disabled) && !Camera.allCameras.Contains(inactive), "allCameras members");
        var buffer = new Camera[5];
        Ensure(Camera.GetAllCameras(buffer) == 3 && buffer[0] is not null && buffer[3] is null, "GetAllCameras fills the array and returns the count");
        Throws<ArgumentException>(() => Camera.GetAllCameras(new Camera[2]), "too small an array", "smaller than allCamerasCount");
        Throws<ArgumentNullException>(() => Camera.GetAllCameras(null!), "null array", "cameras");
        inactive.gameObject.SetActive(true);
        Ensure(Camera.allCamerasCount == 4, "activating an object adds its camera");
    }

    private static void CopyFromCopiesEverySetting()
    {
        var source = NewCamera();
        source.orthographic = false;
        source.fieldOfView = 33f;
        source.orthographicSize = 8f;
        source.nearClipPlane = 0.5f;
        source.farClipPlane = 500f;
        source.depth = 4;
        source.clearFlags = CameraClearFlags.Skybox;
        source.backgroundColor = new Color(0.25f, 0.5f, 0.75f, 1f);
        source.cullingMask = 0b110;
        source.rect = new Rect(0.1f, 0.2f, 0.3f, 0.4f);
        source.depthTextureMode = DepthTextureMode.Depth;
        source.allowHDR = false;
        source.allowMSAA = false;
        source.allowDynamicResolution = true;
        source.useOcclusionCulling = false;
        source.targetDisplay = 2;
        source.aspect = 1.25f;
        source.renderingPath = RenderingPath.DeferredShading;
        source.opaqueSortMode = OpaqueSortMode.FrontToBack;
        source.usePhysicalProperties = true;
        source.layerCullDistances = Enumerable.Range(0, 32).Select(i => (float)i).ToArray();
        var texture = new RenderTexture(64, 64, 0);
        source.targetTexture = texture;
        var custom = Matrix4x4.Perspective(70f, 1.5f, 0.2f, 50f);
        source.projectionMatrix = custom;
        var copy = NewCamera();
        copy.enabled = false;
        copy.CopyFrom(source);
        Ensure(!copy.enabled, "CopyFrom keeps whether the camera is enabled");
        Ensure(!copy.orthographic && copy.fieldOfView == 33f && copy.orthographicSize == 8f, "lens copied");
        Ensure(copy.nearClipPlane == 0.5f && copy.farClipPlane == 500f && copy.depth == 4, "clip planes and depth copied");
        Ensure(copy.clearFlags == CameraClearFlags.Skybox && copy.backgroundColor == new Color(0.25f, 0.5f, 0.75f, 1f), "clear settings copied");
        Ensure(copy.cullingMask == 0b110 && copy.rect == new Rect(0.1f, 0.2f, 0.3f, 0.4f), "culling mask and rect copied");
        Ensure(copy.depthTextureMode == DepthTextureMode.Depth && !copy.allowHDR && !copy.allowMSAA && copy.allowDynamicResolution && !copy.useOcclusionCulling, "render flags copied");
        Ensure(copy.targetDisplay == 2 && copy.targetTexture == texture, "display and target copied");
        Ensure(copy.aspect == 1.25f, "aspect override copied");
        Ensure(copy.renderingPath == RenderingPath.DeferredShading && copy.opaqueSortMode == OpaqueSortMode.FrontToBack && copy.usePhysicalProperties, "stored settings copied");
        Ensure(copy.layerCullDistances.SequenceEqual(source.layerCullDistances), "layer cull distances copied");
        Ensure(copy.projectionMatrix == custom, "custom projection copied");
        source.layerCullDistances = new float[32];
        Ensure(copy.layerCullDistances[5] == 5f, "the copy does not share the array");
        source.ResetProjectionMatrix();
        copy.CopyFrom(source);
        Ensure(copy.projectionMatrix != custom, "copying a derived projection clears a custom one");
        Throws<ArgumentNullException>(() => copy.CopyFrom(null!), "null source", "other");
    }

    private static void StoredSettings()
    {
        var camera = NewCamera();
        Ensure(camera.renderingPath == RenderingPath.UsePlayerSettings && camera.actualRenderingPath == RenderingPath.Forward, "default rendering path resolves to Forward");
        camera.renderingPath = RenderingPath.VertexLit;
        Ensure(camera.actualRenderingPath == RenderingPath.VertexLit, "explicit rendering path is the actual one");
        Ensure(camera.opaqueSortMode == OpaqueSortMode.Default && !camera.usePhysicalProperties, "stored defaults");
        Ensure(camera.layerCullDistances.Length == 32 && camera.layerCullDistances.All(distance => distance == 0f), "layer cull distances default to zeros");
        camera.layerCullDistances[1] = 9f;
        Ensure(camera.layerCullDistances[1] == 0f, "the getter returns a copy");
        Throws<ArgumentException>(() => camera.layerCullDistances = new float[31], "wrong length", "32 floats");
        camera.layerCullDistances = null!;
        Ensure(camera.layerCullDistances.Length == 32, "null clears the distances");
        Ensure(!camera.stereoEnabled && camera.stereoActiveEye == Camera.MonoOrStereoscopicEye.Mono && camera.stereoTargetEye == StereoTargetEyeMask.Both, "stereo defaults");
        Ensure(!camera.isProcessingRenderRequest && !camera.useJitteredProjectionMatrixForTransparentRendering, "render request defaults");
        Ensure(camera.cameraType == CameraType.Game, "camera type defaults to Game");
    }

    private static void UnsupportedMembers()
    {
        var camera = NewCamera();
        var matrix = Matrix4x4.identity;
        var culling = new ScriptableCullingParameters();
        var members = new (string, Action)[]
        {
            ("TryGetCullingParameters", () => camera.TryGetCullingParameters(ref culling)),
            ("GetStereoNonJitteredProjectionMatrix", () => camera.GetStereoNonJitteredProjectionMatrix(Camera.StereoscopicEye.Left)),
            ("GetStereoViewMatrix", () => camera.GetStereoViewMatrix(Camera.StereoscopicEye.Left)),
            ("GetStereoProjectionMatrix", () => camera.GetStereoProjectionMatrix(Camera.StereoscopicEye.Left)),
            ("SetStereoProjectionMatrix", () => camera.SetStereoProjectionMatrix(Camera.StereoscopicEye.Left, matrix)),
            ("SetStereoViewMatrix", () => camera.SetStereoViewMatrix(Camera.StereoscopicEye.Right, matrix)),
            ("CopyStereoDeviceProjectionMatrixToNonJittered", () => camera.CopyStereoDeviceProjectionMatrixToNonJittered(Camera.StereoscopicEye.Left)),
            ("ResetStereoProjectionMatrices", () => camera.ResetStereoProjectionMatrices()),
            ("ResetStereoViewMatrices", () => camera.ResetStereoViewMatrices()),
        };
        foreach (var (name, call) in members) NotSupported(call, name, "Camera." + name);
    }
}
