namespace UnityEngine
{
    public enum RenderMode { ScreenSpaceOverlay, ScreenSpaceCamera, WorldSpace }
    public class Canvas : Behaviour
    {
        public RenderMode renderMode
        {
            get => Fields.GetProperty("render_mode").GetString() switch { "screen_space_overlay" => RenderMode.ScreenSpaceOverlay, "screen_space_camera" => RenderMode.ScreenSpaceCamera, "world_space" => RenderMode.WorldSpace, var v => throw new NotSupportedException($"Canvas.renderMode '{v}'") };
            set => SetField("render_mode", value switch { RenderMode.ScreenSpaceOverlay => "screen_space_overlay", RenderMode.ScreenSpaceCamera => "screen_space_camera", RenderMode.WorldSpace => "world_space", _ => throw new ArgumentOutOfRangeException(nameof(value)) });
        }
        public Camera? worldCamera
        {
            get { var camera = Fields.GetProperty("camera"); return camera.ValueKind == System.Text.Json.JsonValueKind.Null ? null : (Camera)NativeIdentity.Resolve(camera.GetProperty("$ref").GetUInt64()); }
            set => SetField("camera", value is null ? null : new Dictionary<string, ulong> { ["$ref"] = value.NativeHandle });
        }
        public float planeDistance { get => Fields.GetProperty("plane_distance").GetSingle(); set => SetField("plane_distance", value); }
        public int sortingOrder { get => Fields.GetProperty("sorting_order").GetInt32(); set => SetField("sorting_order", value); }
    }
}
namespace UnityEngine.UI
{
    public class CanvasScaler : UnityEngine.Behaviour
    {
        public enum ScaleMode { ConstantPixelSize, ScaleWithScreenSize, ConstantPhysicalSize }
        public ScaleMode uiScaleMode
        {
            get => Fields.GetProperty("scale_mode").GetString() switch { "constant_pixel_size" => ScaleMode.ConstantPixelSize, "scale_with_screen_size" => ScaleMode.ScaleWithScreenSize, var v => throw new NotSupportedException($"CanvasScaler.uiScaleMode '{v}'") };
            set => SetField("scale_mode", value switch { ScaleMode.ConstantPixelSize => "constant_pixel_size", ScaleMode.ScaleWithScreenSize => "scale_with_screen_size", _ => throw new NotSupportedException($"CanvasScaler.uiScaleMode '{value}'") });
        }
        public float scaleFactor { get => Fields.GetProperty("scale_factor").GetSingle(); set => SetField("scale_factor", value); }
        public float matchWidthOrHeight { get => Fields.GetProperty("match_width_or_height").GetSingle(); set => SetField("match_width_or_height", value); }
        public UnityEngine.Vector2 referenceResolution
        {
            get { var v = Fields.GetProperty("reference_resolution"); return new(v[0].GetSingle(), v[1].GetSingle()); }
            set => SetField("reference_resolution", new[] { value.x, value.y });
        }
    }
}
