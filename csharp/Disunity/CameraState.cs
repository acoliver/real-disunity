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

    /// <summary>The enabled cameras on active objects in the order a pipeline renders them: ascending <see cref="depth"/>,
    /// cameras of equal depth in creation order.</summary>
    internal static List<Camera> RenderOrder() => EnabledCameras().OrderBy(camera => camera.depth).ToList();

    private static IEnumerable<Camera> EnabledCameras() => DisunityHost.ActiveObjects()
        .SelectMany(owner => DisunityHost.Components(owner).OfType<Camera>())
        .Where(camera => camera.enabled);

    /// <summary>
    /// Makes this camera's settings match <paramref name="other"/>: lens, clear mode, depth, culling,
    /// rectangle, target (texture or buffers), custom matrices and the stored settings. Whether the camera is enabled stays as it is.
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
        var buffers = WorldGraphics.ReadCameraBuffers(other.Handle);
        WorldGraphics.WriteCameraBuffers(Handle, buffers?.Colors ?? [], buffers?.Depth ?? 0);
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

    /// <summary>With a <see cref="targetTexture"/> records the camera's render into it as offscreen work in the world, which the
    /// host executes with the camera's culling mask, clear flags and pose as they are then; the texture is created if it is
    /// not. Without a target texture the call would draw to the screen, which the host draws from the frame's selected camera
    /// and offers no explicit request for, so it throws <see cref="NotSupportedException"/>. While a render pipeline is being
    /// called by the host, the call renders through it instead: the pipeline's Render
    /// runs once with this camera alone and no <c>CameraRender</c> is recorded. Called from inside a pipeline's Render it
    /// throws <see cref="InvalidOperationException"/>, the engine's choice. A camera with <see cref="SetTargetBuffers(RenderBuffer, RenderBuffer)"/>
    /// buffers renders only through a pipeline, because the camera's own render has one target; outside one the call throws
    /// <see cref="NotSupportedException"/>. A destroyed camera throws.</summary>
    public void Render()
    {
        ThrowIfDestroyed();
        if (DisunityHost.TryRenderThroughPipeline(this)) return;
        if (targetTexture is not { } target)
        {
            if (WorldGraphics.ReadCameraBuffers(Handle) is not null)
            {
                throw new NotSupportedException("Camera.Render with SetTargetBuffers outside a render pipeline is not supported: "
                    + "the camera's own render has one target; render through a pipeline, or set targetTexture");
            }
            throw new NotSupportedException("Camera.Render without a targetTexture draws to the screen, which is not supported: "
                + "the host draws the screen from the frame's selected camera; set targetTexture to render into a texture");
        }
        if (!target.IsCreated()) target.Create();
        WorldGraphics.RecordCameraRender(Handle, target.WorldId);
    }

    /// <summary>Completes without drawing and logs a note the first time it is called in a world. A null shader throws.</summary>
    public void RenderWithShader(Shader shader, string replacementTag)
    {
        ArgumentNullException.ThrowIfNull(shader);
        ThrowIfDestroyed();
        UndrawnCalls.Note("Camera.RenderWithShader");
    }

    /// <summary>The most color buffers a camera draws into, which the world and wgpu share.</summary>
    private const int MaxColorBuffers = 8;

    /// <summary>Makes the camera draw into the color buffer of <paramref name="colorBuffer"/>'s render texture and the depth
    /// buffer of <paramref name="depthBuffer"/>'s, which may be another render texture's. It replaces
    /// <see cref="targetTexture"/>, which reads null afterwards, and setting <see cref="targetTexture"/> replaces the buffers.
    /// A destroyed camera throws <see cref="InvalidOperationException"/>; see the array overload for the buffers it refuses.</summary>
    public void SetTargetBuffers(RenderBuffer colorBuffer, RenderBuffer depthBuffer)
    {
        ThrowIfDestroyed();
        WriteTargetBuffers([colorBuffer], depthBuffer);
    }

    /// <summary>Makes the camera draw into the color buffers of one to eight render textures, which a pass fills from the fragment
    /// stage's outputs in this order, and the depth buffer of <paramref name="depthBuffer"/>'s render texture, which may be none
    /// of theirs or none at all (<c>default</c>). The camera's pipeline then renders into them as it does into the camera
    /// target, and a later pass reads each one as the texture it is. The engine refuses with <see cref="ArgumentException"/> a
    /// color buffer that is not a live render texture's color surface, a depth buffer that is not a live render texture's depth
    /// surface, none or more than eight color buffers, one render texture twice, and buffers of different sizes. A null array
    /// throws <see cref="ArgumentNullException"/> and a destroyed camera <see cref="InvalidOperationException"/>.</summary>
    public void SetTargetBuffers(RenderBuffer[] colorBuffer, RenderBuffer depthBuffer)
    {
        ArgumentNullException.ThrowIfNull(colorBuffer);
        ThrowIfDestroyed();
        WriteTargetBuffers(colorBuffer, depthBuffer);
    }

    private void WriteTargetBuffers(RenderBuffer[] colors, RenderBuffer depth)
    {
        const string member = "Camera.SetTargetBuffers";
        if (colors.Length is 0 or > MaxColorBuffers)
            throw new ArgumentException($"{member}: {colors.Length} color buffers; a camera draws into one to eight", "colorBuffer");
        var textures = colors.Select(buffer => BufferTexture(buffer, surface: 0, member, "colorBuffer")).ToArray();
        var depthTexture = depth.m_RenderTextureInstanceID == 0 ? null : BufferTexture(depth, surface: 1, member, "depthBuffer");
        for (var index = 0; index < textures.Length; index++)
        {
            if (textures.Take(index).Contains(textures[index]))
                throw new ArgumentException($"{member}: color buffer {index} is a render texture that is also an earlier color buffer", "colorBuffer");
            RequireSize(textures[0], textures[index], $"color buffer {index}", member);
        }
        if (depthTexture is not null) RequireSize(textures[0], depthTexture, "the depth buffer", member);
        var all = textures.ToList();
        if (depthTexture is not null) all.Add(depthTexture);
        foreach (var texture in all)
            if (!texture.IsCreated()) texture.Create();
        WorldGraphics.WriteCameraBuffers(Handle, textures.Select(texture => texture.WorldId).ToArray(), depthTexture?.WorldId ?? 0);
    }

    /// <summary>The render texture a buffer names, if it is a live one's surface <paramref name="surface"/> (0 the color, 1 the depth).</summary>
    private static RenderTexture BufferTexture(RenderBuffer buffer, long surface, string member, string parameter)
    {
        var texture = buffer.m_RenderTextureInstanceID == 0 ? null : ObjectRegistry.Find(buffer.m_RenderTextureInstanceID) as RenderTexture;
        if (texture is null || !texture)
            throw new ArgumentException($"{member}: the {parameter} is not a live render texture's buffer", parameter);
        if ((buffer.m_BufferPtr.ToInt64() & 1) != surface)
            throw new ArgumentException($"{member}: the {parameter} is a render texture's {(surface == 0 ? "depth" : "color")} buffer, not its {(surface == 0 ? "color" : "depth")} buffer", parameter);
        return texture;
    }

    private static void RequireSize(RenderTexture first, RenderTexture other, string what, string member)
    {
        if (other.width != first.width || other.height != first.height)
            throw new ArgumentException($"{member}: {what} is {other.width}x{other.height} and the first color buffer is {first.width}x{first.height}");
    }

    private CommandBufferSlots<CameraEvent> CommandBuffers => CommandBufferSlots<CameraEvent>.Of(this);
    public int commandBufferCount => CommandBuffers.Count;
    public void AddCommandBuffer(CameraEvent evt, CommandBuffer buffer) => CommandBuffers.Add(evt, buffer, "Camera.AddCommandBuffer");
    public void RemoveCommandBuffer(CameraEvent evt, CommandBuffer buffer) => CommandBuffers.Remove(evt, buffer, "Camera.RemoveCommandBuffer");
    public void RemoveCommandBuffers(CameraEvent evt) => CommandBuffers.RemoveAll(evt, "Camera.RemoveCommandBuffers");
    public void RemoveAllCommandBuffers() => CommandBuffers.Clear();
    public CommandBuffer[] GetCommandBuffers(CameraEvent evt) => CommandBuffers.Get(evt, "Camera.GetCommandBuffers");
    /// <summary>Fills the parameters from the camera: its culling mask, the six planes of its view volume, the product of its
    /// projection and view matrices, its position, its level of detail parameters, the quality settings' shadow distance
    /// (stopped at the far plane), its layer cull distances, and no culling options (the engine has no light, probe or occlusion
    /// culling). Returns false, with the parameters left empty, for a camera whose view has no pixels or whose matrices are not
    /// finite. A destroyed camera throws <see cref="InvalidOperationException"/>.</summary>
    public bool TryGetCullingParameters(out ScriptableCullingParameters cullingParameters)
    {
        ThrowIfDestroyed();
        return CullingParametersFilled(out cullingParameters);
    }

    /// <summary>The same as <see cref="TryGetCullingParameters(out ScriptableCullingParameters)"/>: the engine has no stereo
    /// rendering, so <paramref name="stereoAware"/> changes nothing.</summary>
    public bool TryGetCullingParameters(bool stereoAware, out ScriptableCullingParameters cullingParameters) =>
        TryGetCullingParameters(out cullingParameters);

    public Matrix4x4 GetStereoNonJitteredProjectionMatrix(StereoscopicEye eye) => throw Unsupported(nameof(GetStereoNonJitteredProjectionMatrix));
    public Matrix4x4 GetStereoViewMatrix(StereoscopicEye eye) => throw Unsupported(nameof(GetStereoViewMatrix));
    public Matrix4x4 GetStereoProjectionMatrix(StereoscopicEye eye) => throw Unsupported(nameof(GetStereoProjectionMatrix));
    public void SetStereoProjectionMatrix(StereoscopicEye eye, Matrix4x4 matrix) => throw Unsupported(nameof(SetStereoProjectionMatrix));
    public void SetStereoViewMatrix(StereoscopicEye eye, Matrix4x4 matrix) => throw Unsupported(nameof(SetStereoViewMatrix));
    public void CopyStereoDeviceProjectionMatrixToNonJittered(StereoscopicEye eye) => throw Unsupported(nameof(CopyStereoDeviceProjectionMatrixToNonJittered));
    public void ResetStereoProjectionMatrices() => throw Unsupported(nameof(ResetStereoProjectionMatrices));
    public void ResetStereoViewMatrices() => throw Unsupported(nameof(ResetStereoViewMatrices));
}
