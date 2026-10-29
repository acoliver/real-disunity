using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>Unity's clear modes. Depth and Nothing need a lower camera and fail at presentation.</summary>
public enum CameraClearFlags { Skybox = 1, Color = 2, SolidColor = 2, Depth = 3, Nothing = 4 }

/// <summary>
/// A native camera. Each setting that changes what a frame shows is read from and written to the
/// native camera, and a write the native side rejects leaves every setting unchanged. A frame
/// presents orthographic cameras that fill the screen; a perspective lens, custom matrices, a
/// target texture, another display or a partial <see cref="rect"/> are stored and fail when a
/// frame is prepared. A camera added to an object starts orthographic, as the 2D games expect;
/// scene data sets the lens the object serialized.
/// </summary>
public sealed partial class Camera : NativeComponent
{
    public enum MonoOrStereoscopicEye { Left = 0, Right = 1, Mono = 2 }
    public enum StereoscopicEye { Left = 0, Right = 1 }

    private static readonly object TargetLock = new();
    private static readonly Dictionary<int, WeakReference<RenderTexture>> TargetTextures = new();

    public enum GateFitMode { Vertical = 1, Horizontal = 2, Fill = 3, Overscan = 4, None = 0 }

    private unsafe DisunityCamera Read()
    {
        DisunityCamera value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Write(DisunityCamera value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_write(DisunityHost.WorldHandle, Handle, &value));
    private void Update(Func<DisunityCamera, DisunityCamera> change) => Write(change(Read()));

    public bool enabled { get => Read().enabled == 1; set => Update(c => { c.enabled = value ? 1u : 0u; return c; }); }
    public CameraClearFlags clearFlags { get => (CameraClearFlags)Read().clear_flags; set => Update(c => { c.clear_flags = (uint)value; return c; }); }
    public Color backgroundColor
    {
        get { var c = Read(); return new Color(c.r, c.g, c.b, c.a); }
        set => Update(c => { (c.r, c.g, c.b, c.a) = (value.r, value.g, value.b, value.a); return c; });
    }
    public bool orthographic { get => Read().orthographic == 1; set => Update(c => { c.orthographic = value ? 1u : 0u; return c; }); }
    public float orthographicSize { get => Read().orthographic_size; set => Update(c => { c.orthographic_size = value; return c; }); }
    /// <summary>Vertical field of view in degrees, between 0 and 180 exclusive. It is kept while the camera is orthographic.</summary>
    public float fieldOfView { get => Read().field_of_view; set => Update(c => { c.field_of_view = value; return c; }); }
    public float nearClipPlane { get => Read().near; set => Update(c => { c.near = value; return c; }); }
    public float farClipPlane { get => Read().far; set => Update(c => { c.far = value; return c; }); }
    /// <summary>Draw order between cameras. Native depth is whole, so fractional values are rejected.</summary>
    public float depth
    {
        get => Read().depth;
        set
        {
            if (value != MathF.Round(value)) throw new ArgumentException("camera depth must be a whole number", nameof(value));
            Update(c => { c.depth = checked((int)value); return c; });
        }
    }

    /// <summary>One bit per layer; the camera draws sprites, tiles and world-space text on the layers set.</summary>
    public int cullingMask { get => unchecked((int)Read().culling_mask); set => Update(c => { c.culling_mask = unchecked((uint)value); return c; }); }
    public DepthTextureMode depthTextureMode { get => (DepthTextureMode)Read().depth_texture_mode; set => Update(c => { c.depth_texture_mode = (uint)value; return c; }); }
    public bool allowHDR { get => Read().allow_hdr == 1; set => Update(c => { c.allow_hdr = value ? 1u : 0u; return c; }); }
    public bool allowMSAA { get => Read().allow_msaa == 1; set => Update(c => { c.allow_msaa = value ? 1u : 0u; return c; }); }
    public bool allowDynamicResolution { get => Read().allow_dynamic_resolution == 1; set => Update(c => { c.allow_dynamic_resolution = value ? 1u : 0u; return c; }); }
    public bool useOcclusionCulling { get => Read().use_occlusion_culling == 1; set => Update(c => { c.use_occlusion_culling = value ? 1u : 0u; return c; }); }
    public int targetDisplay { get => (int)Read().target_display; set => Update(c => { c.target_display = checked((uint)value); return c; }); }
    public CameraType cameraType { get => (CameraType)Read().camera_type; set => Update(c => { c.camera_type = (uint)value; return c; }); }

    /// <summary>The part of the target the camera draws into, in normalized coordinates with the origin at the bottom left.</summary>
    public Rect rect
    {
        get { var c = Read(); return new Rect(c.viewport_x, c.viewport_y, c.viewport_width, c.viewport_height); }
        set => Update(c => { (c.viewport_x, c.viewport_y, c.viewport_width, c.viewport_height) = (value.x, value.y, value.width, value.height); return c; });
    }

    /// <summary>The same rectangle in pixels of the target: the screen, or the target texture when one is set.</summary>
    public Rect pixelRect
    {
        get
        {
            var (width, height) = TargetSize();
            var r = rect;
            return new Rect(r.x * width, r.y * height, r.width * width, r.height * height);
        }
        set
        {
            var (width, height) = TargetSize();
            if (width <= 0 || height <= 0) throw new InvalidOperationException("the camera's target has no pixels");
            rect = new Rect(value.x / width, value.y / height, value.width / width, value.height / height);
        }
    }

    // Pixel sizes round to the nearest whole pixel (halves away from zero).
    public int pixelWidth => (int)MathF.Round(pixelRect.width, MidpointRounding.AwayFromZero);
    public int pixelHeight => (int)MathF.Round(pixelRect.height, MidpointRounding.AwayFromZero);
    /// <summary>Equal to <see cref="pixelWidth"/>: dynamic resolution scaling is not applied.</summary>
    public int scaledPixelWidth => pixelWidth;
    public int scaledPixelHeight => pixelHeight;

    private static (int Width, int Height) ScreenSize() => (Screen.width, Screen.height);

    private (int Width, int Height) TargetSize() =>
        targetTexture is { } texture ? (texture.width, texture.height) : ScreenSize();

    /// <summary>The texture the camera draws into, or null for the screen. A frame cannot present a target texture yet.</summary>
    public RenderTexture? targetTexture
    {
        get
        {
            var id = Read().target_texture;
            if (id == 0) return null;
            lock (TargetLock)
            {
                if (!TargetTextures.TryGetValue(unchecked((int)(uint)id), out var weak) || !weak.TryGetTarget(out var texture)) return null;
                return texture ? texture : null;
            }
        }
        set
        {
            if (value is not null && !value) throw new ArgumentException("the target texture was destroyed", nameof(value));
            var id = value is null ? 0ul : unchecked((ulong)(uint)value.GetInstanceID());
            if (value is not null) lock (TargetLock) TargetTextures[value.GetInstanceID()] = new WeakReference<RenderTexture>(value);
            Update(c => { c.target_texture = id; return c; });
        }
    }

    /// <summary>The texture the camera draws into; the same as <see cref="targetTexture"/>.</summary>
    public RenderTexture? activeTexture => targetTexture;
}
