using Disunity.Native;
using Disunity.Hosting;

namespace UnityEngine;

public sealed partial class Canvas
{
    private const float DefaultPlaneDistance = 100f;

    private Camera? _eventCamera;
    private bool _cameraModePending;

    /// <summary>
    /// How the canvas is drawn. A canvas set to ScreenSpaceCamera with no camera yet draws as an overlay and
    /// reports ScreenSpaceCamera; it renders through the camera as soon as <see cref="worldCamera"/> is set.
    /// </summary>
    public RenderMode renderMode
    {
        get
        {
            var mode = (RenderMode)Read().mode;
            return _cameraModePending && mode == RenderMode.ScreenSpaceOverlay ? RenderMode.ScreenSpaceCamera : mode;
        }
        set
        {
            var c = Read();
            var inCameraMode = c.mode == (uint)RenderMode.ScreenSpaceCamera;
            _cameraModePending = false;
            if (value == RenderMode.ScreenSpaceCamera && inCameraMode) return;
            if (value == RenderMode.ScreenSpaceCamera) { EnterCameraMode(c); return; }
            c.mode = (uint)value;
            Write(c);
        }
    }

    private void EnterCameraMode(DisunityCanvas c)
    {
        if (_eventCamera != null)
        {
            DisunityHost.RenderThroughCamera(this, _eventCamera, c.plane_distance > 0 ? c.plane_distance : DefaultPlaneDistance);
            return;
        }
        c.mode = (uint)RenderMode.ScreenSpaceOverlay;
        Write(c);
        _cameraModePending = true;
    }

    /// <summary>Sorts this canvas by its own layer and order instead of its parent's. Stored; sorting does not read it yet.</summary>
    public bool overrideSorting { get => Read().override_sorting == 1; set { var c = Read(); c.override_sorting = value ? 1u : 0u; Write(c); } }

    public int sortingLayerID { get => Read().sorting_layer; set { var c = Read(); c.sorting_layer = value; Write(c); } }

    /// <summary>The engine has no sorting layer table, so only the Default layer (ID 0) has a name.</summary>
    /// <exception cref="NotSupportedException">The layer is not Default, or the name is not "Default".</exception>
    public string sortingLayerName
    {
        get => sortingLayerID == 0 ? "Default" : throw new NotSupportedException("Canvas.sortingLayerName for a sorting layer other than Default is not supported yet");
        set
        {
            if (value != "Default") throw new NotSupportedException("Canvas.sortingLayerName other than Default is not supported yet");
            sortingLayerID = 0;
        }
    }

    /// <summary>The display the canvas targets. Stored; the engine has one display.</summary>
    public int targetDisplay { get => (int)Read().target_display; set { var c = Read(); c.target_display = checked((uint)value); Write(c); } }

    public AdditionalCanvasShaderChannels additionalShaderChannels
    {
        get => (AdditionalCanvasShaderChannels)Read().additional_shader_channels;
        set { var c = Read(); c.additional_shader_channels = (uint)value; Write(c); }
    }

    /// <summary>Pixels per world unit for sprites on this canvas. Stored; layout does not read it.</summary>
    public float referencePixelsPerUnit { get => Read().reference_pixels_per_unit; set { var c = Read(); c.reference_pixels_per_unit = value; Write(c); } }

    /// <summary>The camera a ScreenSpaceCamera canvas renders through, or for another mode the event camera set on it.</summary>
    /// <exception cref="NotSupportedException">Set to null on a ScreenSpaceCamera canvas, which cannot draw without a camera.</exception>
    public Camera? worldCamera
    {
        get
        {
            var c = Read();
            return c.mode == (uint)RenderMode.ScreenSpaceCamera ? DisunityHost.ResolveObject(c.camera).GetComponent<Camera>() : _eventCamera;
        }
        set
        {
            var c = Read();
            if (c.mode == (uint)RenderMode.ScreenSpaceCamera)
            {
                if (value == null) throw new NotSupportedException("Canvas.worldCamera = null on a ScreenSpaceCamera canvas is not supported");
                DisunityHost.RenderThroughCamera(this, value, c.plane_distance);
                return;
            }
            _eventCamera = value;
            if (_cameraModePending && value != null)
            {
                _cameraModePending = false;
                DisunityHost.RenderThroughCamera(this, value, c.plane_distance > 0 ? c.plane_distance : DefaultPlaneDistance);
            }
        }
    }

    /// <summary>True when no enabled canvas sits above this one.</summary>
    public bool isRootCanvas => ReferenceEquals(rootCanvas, this);

    /// <summary>The topmost enabled canvas on the chain of parents, or this canvas when none is above it.</summary>
    public Canvas rootCanvas
    {
        get
        {
            var root = this;
            for (var parent = transform.parent; parent != null; parent = parent.parent)
            {
                var above = parent.GetComponent<Canvas>();
                if (above != null && above.enabled) root = above;
            }
            return root;
        }
    }

    /// <summary>The screen for a screen-space canvas; in world space the world camera's pixels, or the screen without one.</summary>
    public Rect pixelRect
    {
        get
        {
            var camera = (RenderMode)Read().mode == RenderMode.WorldSpace ? worldCamera : null;
            return camera != null ? camera.pixelRect : new Rect(0, 0, Screen.width, Screen.height);
        }
    }
}
