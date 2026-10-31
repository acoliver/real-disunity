using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public enum RenderMode { ScreenSpaceOverlay, ScreenSpaceCamera, WorldSpace }
public sealed partial class Canvas : Behaviour
{
    internal unsafe DisunityCanvas Read()
    {
        DisunityCanvas value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    internal unsafe void Write(DisunityCanvas value) => DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_write(DisunityHost.WorldHandle, Handle, &value));
    private protected override bool EnabledState { get => Read().enabled == 1; set { var c = Read(); c.enabled = value ? 1u : 0u; Write(c); } }
    public int sortingOrder { get => Read().sorting_order; set { var c = Read(); c.sorting_order = value; Write(c); } }
    /// <summary>On a screen-space canvas, moves each text so its first glyph starts on a whole
    /// screen pixel, as Unity's Text does. World-space canvases ignore it.</summary>
    public unsafe bool pixelPerfect
    {
        get
        {
            uint value;
            DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_pixel_perfect_read(DisunityHost.WorldHandle, Handle, &value));
            return value == 1;
        }
        set => DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_pixel_perfect_write(DisunityHost.WorldHandle, Handle, value ? 1u : 0u));
    }
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
}
public sealed class Font
{
    internal readonly ulong Handle;
    internal readonly ulong World;
    internal Font(ulong handle, ulong world) { Handle = handle; World = world; }

    private static readonly object RebuildLock = new();
    private static Action<Font>? _textureRebuilt;

    /// <summary>Raised when a dynamic font's atlas is rebuilt. The engine rebuilds no atlas, so it never raises this; UI
    /// text still adds and removes its handler as it tracks the font.</summary>
    public static event Action<Font>? textureRebuilt
    {
        add { lock (RebuildLock) _textureRebuilt += value; }
        remove { lock (RebuildLock) _textureRebuilt -= value; }
    }
}

/// <summary>A UI element that decides whether a screen point inside its rectangle counts as a hit. The engine sends
/// no UI pointer events yet, so nothing calls it.</summary>
public interface ICanvasRaycastFilter
{
    bool IsRaycastLocationValid(Vector2 sp, Camera eventCamera);
}
