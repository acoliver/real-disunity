using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public sealed class RectTransform : Transform
{
    public RectTransform() { }
    private unsafe DisunityRect Read()
    {
        DisunityRect value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_rect_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    /// <summary>Sets the whole layout at once, so no intermediate state has min above max.</summary>
    internal void SetLayout(Vector2 anchorMin, Vector2 anchorMax, Vector2 pivot, Vector2 sizeDelta, Vector2 anchoredPosition)
    {
        var r = Read();
        (r.min_x, r.min_y, r.max_x, r.max_y) = (anchorMin.x, anchorMin.y, anchorMax.x, anchorMax.y);
        (r.pivot_x, r.pivot_y, r.size_x, r.size_y) = (pivot.x, pivot.y, sizeDelta.x, sizeDelta.y);
        (r.position_x, r.position_y) = (anchoredPosition.x, anchoredPosition.y);
        Write(r);
    }
    private unsafe void Write(DisunityRect value) => DisunityHost.ThrowIfError(NativeBindings.disunity_rect_write(DisunityHost.WorldHandle, Handle, &value));
    public Vector2 anchorMin { get { var r = Read(); return new(r.min_x, r.min_y); } set { var r = Read(); r.min_x = value.x; r.min_y = value.y; Write(r); } }
    public Vector2 anchorMax { get { var r = Read(); return new(r.max_x, r.max_y); } set { var r = Read(); r.max_x = value.x; r.max_y = value.y; Write(r); } }
    public Vector2 pivot { get { var r = Read(); return new(r.pivot_x, r.pivot_y); } set { var r = Read(); r.pivot_x = value.x; r.pivot_y = value.y; Write(r); } }
    public Vector2 sizeDelta { get { var r = Read(); return new(r.size_x, r.size_y); } set { var r = Read(); r.size_x = value.x; r.size_y = value.y; Write(r); } }
    public Vector2 anchoredPosition { get { var r = Read(); return new(r.position_x, r.position_y); } set { var r = Read(); r.position_x = value.x; r.position_y = value.y; Write(r); } }
    public unsafe Rect rect
    {
        get
        {
            DisunityRect r;
            DisunityHost.ThrowIfError(NativeBindings.disunity_ui_rect(DisunityHost.WorldHandle, gameObject.NativeHandle, DisunityHost.ViewportWidth, DisunityHost.ViewportHeight, &r));
            return new(r.position_x, r.position_y, r.size_x, r.size_y);
        }
    }
}
