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
    internal unsafe void Write(DisunityCanvas value)
    {
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_write(DisunityHost.WorldHandle, Handle, &value));
        RectDimensionMessages.MarkChanged();
    }
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
            RectDimensionMessages.MarkChanged();
        }
    }
}
public sealed partial class Font : Object
{
    internal readonly ulong Handle;
    internal readonly ulong World;
    internal Font(ulong handle, ulong world) { Handle = handle; World = world; }

    /// <summary>The font file this font was made from, which <see cref="UnityEngine.TextCore.LowLevel.FontEngine"/>
    /// loads a face from; null for a font made without one, which stays unusable there.</summary>
    internal byte[]? FontData { get; init; }

    /// <summary>A font a player stores: it has the font file and no native UI registration, so only
    /// <see cref="UnityEngine.TextCore.LowLevel.FontEngine"/> uses it. A font with an empty file has no data.</summary>
    internal Font(string name, byte[] file) : this(0, 0)
    {
        this.name = name;
        FontData = file.Length == 0 ? null : file;
    }

    /// <summary>True when the engine draws this font's glyphs on demand from font data, which is the case for a font that
    /// holds its font file (registered by the host or stored by a player). A font with no file has only imported glyph data
    /// and is not dynamic. The engine has no system fonts, so no font is made from an OS font.</summary>
    public bool dynamic => FontData is not null;

    private Material? _material;
    private bool _materialIsOwn;
    private Texture2D? _atlas;

    /// <summary>The material UI text draws this font with: a copy of the default UI material named after the font,
    /// made on first use and kept until destroyed. Its main texture is the font's glyph atlas once a glyph was drawn;
    /// setting the material replaces it, and the engine then leaves the replacement's texture alone.</summary>
    public Material material
    {
        get
        {
            if (_material) return _material!;
            _materialIsOwn = true;
            return _material = new Material(Canvas.GetDefaultCanvasMaterial()) { name = name, mainTexture = _atlas };
        }
        set
        {
            _material = value;
            _materialIsOwn = false;
        }
    }

    /// <summary>Makes the atlas texture the main texture of the material the font made.</summary>
    internal void AdoptAtlas(Texture2D? atlas)
    {
        _atlas = atlas;
        if (_materialIsOwn && _material) _material!.mainTexture = atlas;
    }

    private static readonly object RebuildLock = new();
    private static Action<Font>? _textureRebuilt;

    /// <summary>Raised when a dynamic font's atlas texture is replaced: after it grew, or was made again in a new world.
    /// UI text adds and removes its handler as it tracks the font, and rebuilds its geometry when it is raised.</summary>
    public static event Action<Font>? textureRebuilt
    {
        add { lock (RebuildLock) _textureRebuilt += value; }
        remove { lock (RebuildLock) _textureRebuilt -= value; }
    }

    private void RaiseTextureRebuilt()
    {
        Action<Font>? handlers;
        lock (RebuildLock) handlers = _textureRebuilt;
        handlers?.Invoke(this);
    }
}

/// <summary>A UI element that decides whether a screen point inside its rectangle counts as a hit. The engine sends
/// no UI pointer events yet, so nothing calls it.</summary>
public interface ICanvasRaycastFilter
{
    bool IsRaycastLocationValid(Vector2 sp, Camera eventCamera);
}
