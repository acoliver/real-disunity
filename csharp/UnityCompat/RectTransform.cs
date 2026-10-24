namespace UnityEngine;

public sealed class RectTransform : Transform
{
    public Vector2 anchorMin { get => ReadPair("anchor_min"); set => SetPair("anchor_min", value); }
    public Vector2 anchorMax { get => ReadPair("anchor_max"); set => SetPair("anchor_max", value); }
    public Vector2 pivot { get => ReadPair("pivot"); set => SetPair("pivot", value); }
    public Vector2 sizeDelta { get => ReadPair("size_delta"); set => SetPair("size_delta", value); }
    public Vector2 anchoredPosition { get => ReadPair("anchored_position"); set => SetPair("anchored_position", value); }
    public Vector2 offsetMin { get => ReadPair("offset_min"); set => SetPair("offset_min", value); }
    public Vector2 offsetMax { get => ReadPair("offset_max"); set => SetPair("offset_max", value); }
    public Rect rect
    {
        get
        {
            using var result = NativeWorld.Request("ui.rect", ("object", gameObject.NativeHandle));
            var data = NativeWorld.Data(result);
            var bounds = data.GetProperty("local_rect");
            var size = data.GetProperty("size");
            return new Rect(bounds[0].GetSingle(), bounds[1].GetSingle(), size[0].GetSingle(), size[1].GetSingle());
        }
    }
    private Vector2 ReadPair(string field) { var p = Fields.GetProperty(field); return new(p[0].GetSingle(), p[1].GetSingle()); }
    private void SetPair(string field, Vector2 value) => SetField(field, new[] { value.x, value.y });
}
