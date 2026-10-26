using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public enum RenderMode { ScreenSpaceOverlay, ScreenSpaceCamera, WorldSpace }
public sealed class Canvas : NativeComponent
{
    internal unsafe DisunityCanvas Read()
    {
        DisunityCanvas value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    internal unsafe void Write(DisunityCanvas value) => DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_write(DisunityHost.WorldHandle, Handle, &value));
    public RenderMode renderMode { get => (RenderMode)Read().mode; set { var c = Read(); c.mode = (uint)value; Write(c); } }
    public bool enabled { get => Read().enabled == 1; set { var c = Read(); c.enabled = value ? 1u : 0u; Write(c); } }
    public int sortingOrder { get => Read().sorting_order; set { var c = Read(); c.sorting_order = value; Write(c); } }
    /// <summary>
    /// The factor layout is scaled by on the current viewport, including scaling with screen
    /// size. Setting it gives the canvas that constant factor, as setting it in Unity does.
    /// </summary>
    public unsafe float scaleFactor
    {
        get
        {
            float factor;
            DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_scale_factor(DisunityHost.WorldHandle, Handle, DisunityHost.ViewportWidth, DisunityHost.ViewportHeight, &factor));
            return factor;
        }
        set
        {
            var scale = new DisunityCanvasScale { mode = 0, factor = value };
            DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_scale_write(DisunityHost.WorldHandle, Handle, &scale));
        }
    }
    public float planeDistance { get => Read().plane_distance; set { var c = Read(); c.plane_distance = value; Write(c); } }
    /// <summary>The camera a ScreenSpaceCamera canvas renders through; set it with
    /// <see cref="DisunityHost.RenderThroughCamera"/>.</summary>
    public Camera? worldCamera
    {
        get
        {
            var c = Read();
            return c.mode == (uint)RenderMode.ScreenSpaceCamera ? DisunityHost.ResolveObject(c.camera).GetComponent<Camera>() : null;
        }
    }
}
public sealed class Font
{
    internal readonly ulong Handle;
    internal readonly ulong World;
    internal Font(ulong handle, ulong world) { Handle = handle; World = world; }
}
