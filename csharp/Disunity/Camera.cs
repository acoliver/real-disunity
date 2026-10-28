using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>Unity's clear modes. Depth and Nothing need a lower camera and fail at presentation.</summary>
public enum CameraClearFlags { Skybox = 1, Color = 2, SolidColor = 2, Depth = 3, Nothing = 4 }

/// <summary>
/// A native orthographic camera. Each property reads or writes the native camera; a write the
/// native side rejects leaves every setting unchanged. Perspective projection is unsupported.
/// </summary>
public sealed class Camera : NativeComponent
{
    public enum MonoOrStereoscopicEye { Left = 0, Right = 1, Mono = 2 }

    public enum GateFitMode { Vertical = 1, Horizontal = 2, Fill = 3, Overscan = 4, None = 0 }

    private unsafe DisunityCamera Read()
    {
        DisunityCamera value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Update(Func<DisunityCamera, DisunityCamera> change)
    {
        var value = change(Read());
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_write(DisunityHost.WorldHandle, Handle, &value));
    }
    public bool enabled { get => Read().enabled == 1; set => Update(c => { c.enabled = value ? 1u : 0u; return c; }); }
    public CameraClearFlags clearFlags { get => (CameraClearFlags)Read().clear_flags; set => Update(c => { c.clear_flags = (uint)value; return c; }); }
    public Color backgroundColor
    {
        get { var c = Read(); return new Color(c.r, c.g, c.b, c.a); }
        set => Update(c => { (c.r, c.g, c.b, c.a) = (value.r, value.g, value.b, value.a); return c; });
    }
    public bool orthographic { get => Read().orthographic == 1; set => Update(c => { c.orthographic = value ? 1u : 0u; return c; }); }
    public float orthographicSize { get => Read().orthographic_size; set => Update(c => { c.orthographic_size = value; return c; }); }
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
}
