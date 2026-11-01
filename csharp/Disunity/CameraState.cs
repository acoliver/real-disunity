using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// Camera settings with no effect on a frame the engine can draw, kept on the managed camera, the
/// cameras of a scene, and the members that need a renderer the engine does not have.
/// </summary>
public sealed partial class Camera
{
    private const int LayerCount = 32;
    private float[]? _layerCullDistances;

    /// <summary>Stored. The engine has one built-in path, so <see cref="actualRenderingPath"/> reports Forward for UsePlayerSettings.</summary>
    public RenderingPath renderingPath { get; set; } = RenderingPath.UsePlayerSettings;
    public RenderingPath actualRenderingPath => renderingPath == RenderingPath.UsePlayerSettings ? RenderingPath.Forward : renderingPath;
    public OpaqueSortMode opaqueSortMode { get; set; } = OpaqueSortMode.Default;
    /// <summary>Stored. Physical lens settings (sensor size, focal length) are not modelled, so the projection ignores this flag.</summary>
    public bool usePhysicalProperties { get; set; }
    public bool useJitteredProjectionMatrixForTransparentRendering { get; set; }
    public bool isProcessingRenderRequest => false;
    public StereoTargetEyeMask stereoTargetEye { get; set; } = StereoTargetEyeMask.Both;
    public bool stereoEnabled => false;
    public MonoOrStereoscopicEye stereoActiveEye => MonoOrStereoscopicEye.Mono;

    /// <summary>Stored per layer, 0 meaning the far plane. Getting returns a copy; setting null clears it.</summary>
    public float[] layerCullDistances
    {
        get => _layerCullDistances is { } stored ? (float[])stored.Clone() : new float[LayerCount];
        set
        {
            if (value is not null && value.Length != LayerCount) throw new ArgumentException("Array needs to contain exactly 32 floats for layerCullDistances.", nameof(value));
            _layerCullDistances = (float[]?)value?.Clone();
        }
    }

    /// <summary>The first enabled camera on an active object tagged MainCamera, or null.</summary>
    public static Camera? main
    {
        get
        {
            foreach (var camera in EnabledCameras())
                if (camera.gameObject.CompareTag("MainCamera")) return camera;
            return null;
        }
    }

    /// <summary>Null: the engine does not render through cameras yet, so no camera is ever current.</summary>
    public static Camera? current => null;

    public static int allCamerasCount => EnabledCameras().Count();

    public static Camera[] allCameras => EnabledCameras().ToArray();

    /// <summary>Fills the array with the enabled cameras and returns how many there are; it must be at least that long.</summary>
    public static int GetAllCameras(Camera[] cameras)
    {
        ArgumentNullException.ThrowIfNull(cameras);
        var found = allCameras;
        if (cameras.Length < found.Length) throw new ArgumentException("Passed in array (cameras) is smaller than allCamerasCount", nameof(cameras));
        Array.Copy(found, cameras, found.Length);
        return found.Length;
    }

    private static IEnumerable<Camera> EnabledCameras() => DisunityHost.ActiveObjects()
        .SelectMany(owner => DisunityHost.Components(owner).OfType<Camera>())
        .Where(camera => camera.enabled);

    /// <summary>
    /// Makes this camera's settings match <paramref name="other"/>: lens, clear mode, depth, culling,
    /// rectangle, target, custom matrices and the stored settings. Whether the camera is enabled stays as it is.
    /// </summary>
    public unsafe void CopyFrom(Camera other)
    {
        ArgumentNullException.ThrowIfNull(other);
        if (!other) throw new ArgumentException("the source camera was destroyed", nameof(other));
        var settings = other.Read();
        settings.enabled = Read().enabled;
        var projection = other.ReadMatrix(ProjectionKind);
        var view = other.ReadMatrix(WorldToCameraKind);
        Write(settings);
        WriteMatrix(ProjectionKind, projection);
        WriteMatrix(WorldToCameraKind, view);
        _aspectOverride = other._aspectOverride;
        renderingPath = other.renderingPath;
        opaqueSortMode = other.opaqueSortMode;
        usePhysicalProperties = other.usePhysicalProperties;
        useJitteredProjectionMatrixForTransparentRendering = other.useJitteredProjectionMatrixForTransparentRendering;
        stereoTargetEye = other.stereoTargetEye;
        _layerCullDistances = (float[]?)other._layerCullDistances?.Clone();
    }

    private static NotSupportedException Unsupported(string member) => new($"Camera.{member} is not supported yet");

    private void ThrowIfDestroyed()
    {
        if (!this) throw new InvalidOperationException("the camera was destroyed");
    }

    /// <summary>Completes without drawing until the native draw surface exists (#171). As the engine's choice no render
    /// pipeline callback runs and the target texture is left as it is. A destroyed camera throws.</summary>
    public void Render()
    {
        ThrowIfDestroyed();
        UndrawnCalls.Note("Camera.Render");
    }

    /// <summary>Completes without drawing, as <see cref="Render"/> does. A null shader throws.</summary>
    public void RenderWithShader(Shader shader, string replacementTag)
    {
        ArgumentNullException.ThrowIfNull(shader);
        ThrowIfDestroyed();
        UndrawnCalls.Note("Camera.RenderWithShader");
    }

    /// <summary>Completes without drawing until the native draw surface exists (#171): the buffers are not bound and
    /// <see cref="targetTexture"/> is left as it is. A destroyed camera or a null buffer array throws.</summary>
    public void SetTargetBuffers(RenderBuffer colorBuffer, RenderBuffer depthBuffer)
    {
        ThrowIfDestroyed();
        UndrawnCalls.Note("Camera.SetTargetBuffers");
    }

    public void SetTargetBuffers(RenderBuffer[] colorBuffer, RenderBuffer depthBuffer)
    {
        ArgumentNullException.ThrowIfNull(colorBuffer);
        SetTargetBuffers(default(RenderBuffer), depthBuffer);
    }

    private CommandBufferSlots<CameraEvent> CommandBuffers => CommandBufferSlots<CameraEvent>.Of(this);
    public int commandBufferCount => CommandBuffers.Count;
    public void AddCommandBuffer(CameraEvent evt, CommandBuffer buffer) => CommandBuffers.Add(evt, buffer, "Camera.AddCommandBuffer");
    public void RemoveCommandBuffer(CameraEvent evt, CommandBuffer buffer) => CommandBuffers.Remove(evt, buffer, "Camera.RemoveCommandBuffer");
    public void RemoveCommandBuffers(CameraEvent evt) => CommandBuffers.RemoveAll(evt, "Camera.RemoveCommandBuffers");
    public void RemoveAllCommandBuffers() => CommandBuffers.Clear();
    public CommandBuffer[] GetCommandBuffers(CameraEvent evt) => CommandBuffers.Get(evt, "Camera.GetCommandBuffers");
    public bool TryGetCullingParameters(ref ScriptableCullingParameters cullingParameters) => throw Unsupported(nameof(TryGetCullingParameters));
    public bool TryGetCullingParameters(bool stereoAware, ref ScriptableCullingParameters cullingParameters) => throw Unsupported(nameof(TryGetCullingParameters));
    public Matrix4x4 GetStereoNonJitteredProjectionMatrix(StereoscopicEye eye) => throw Unsupported(nameof(GetStereoNonJitteredProjectionMatrix));
    public Matrix4x4 GetStereoViewMatrix(StereoscopicEye eye) => throw Unsupported(nameof(GetStereoViewMatrix));
    public Matrix4x4 GetStereoProjectionMatrix(StereoscopicEye eye) => throw Unsupported(nameof(GetStereoProjectionMatrix));
    public void SetStereoProjectionMatrix(StereoscopicEye eye, Matrix4x4 matrix) => throw Unsupported(nameof(SetStereoProjectionMatrix));
    public void SetStereoViewMatrix(StereoscopicEye eye, Matrix4x4 matrix) => throw Unsupported(nameof(SetStereoViewMatrix));
    public void CopyStereoDeviceProjectionMatrixToNonJittered(StereoscopicEye eye) => throw Unsupported(nameof(CopyStereoDeviceProjectionMatrixToNonJittered));
    public void ResetStereoProjectionMatrices() => throw Unsupported(nameof(ResetStereoProjectionMatrices));
    public void ResetStereoViewMatrices() => throw Unsupported(nameof(ResetStereoViewMatrices));
}
