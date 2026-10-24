using System.Text.Json;

namespace UnityEngine
{
    public sealed class Sprite : Object
    {
        public string StableId { get; }
        public Rect rect { get; }
        public float pixelsPerUnit { get; }
        public Sprite(string stableId, Rect rect, float pixelsPerUnit)
        {
            StableId = stableId; this.rect = rect; this.pixelsPerUnit = pixelsPerUnit;
            NativeIdentity.MarkData(this, stableId);
        }
    }
    public class SpriteRenderer : Component
    {
        public static Func<JsonElement, Sprite?>? ResolveSprite { get; set; }
        public Sprite? sprite
        {
            get { var value = Fields.GetProperty("sprite"); return value.ValueKind == JsonValueKind.Null ? null : (ResolveSprite ?? throw new InvalidOperationException("sprite resolver is not loaded"))(value); }
            set => SetField("sprite", value is null ? null : new Dictionary<string, object?> { ["$ref"] = value.StableId });
        }
        public Color color
        {
            get { var c = Fields.GetProperty("color"); return new(c[0].GetSingle(), c[1].GetSingle(), c[2].GetSingle(), c[3].GetSingle()); }
            set => SetField("color", new[] { value.r, value.g, value.b, value.a });
        }
    }
    public class Camera : Behaviour { }
    public class Tilemap : Component { }
}
namespace UnityEngine.UI
{
    public class Text : UnityEngine.Behaviour
    {
        public string text { get => Fields.GetProperty("text").GetString()!; set => SetField("text", value); }
    }
}
namespace UnityEngine.InputSystem
{
    public sealed class KeyControl(UnityEngine.KeyCode code)
    {
        public bool isPressed => UnityEngine.Input.GetKey(code);
        public bool wasPressedThisFrame => UnityEngine.Input.GetKeyDown(code);
        public bool wasReleasedThisFrame => UnityEngine.Input.GetKeyUp(code);
    }
    public sealed class Keyboard
    {
        public static Keyboard current { get; } = new();
        public KeyControl aKey { get; } = new(UnityEngine.KeyCode.A);
        public KeyControl dKey { get; } = new(UnityEngine.KeyCode.D);
        public KeyControl wKey { get; } = new(UnityEngine.KeyCode.W);
        public KeyControl sKey { get; } = new(UnityEngine.KeyCode.S);
    }
}
