using System.Text.Json;
using UnityEngine;
using UnityEngine.UI;

namespace Disunity.Hosting;

internal sealed partial class SceneLoader
{
    private static void Layout(RectTransform rect, JsonElement value) => rect.SetLayout(value.Vec2("anchor_min"),
        value.Vec2("anchor_max"), value.Vec2("pivot"), value.Vec2("size_delta"), value.Vec2("anchored_position"));

    private Canvas Canvas(GameObject owner, JsonElement value, string where)
    {
        if (value.Int("target_display") != 0)
            throw new SceneLoadException($"{where}: only display 1 is supported");
        var canvas = owner.AddComponent<Canvas>();
        canvas.sortingLayerID = value.Int("sorting_layer_id");
        canvas.sortingOrder = value.Int("sorting_order");
        var mode = value.Variant<RenderMode>("render_mode");
        var through = value.GetProperty("camera");
        if (mode == RenderMode.ScreenSpaceCamera && through.ValueKind != JsonValueKind.Null)
        {
            var camera = components[through[0].GetInt32()][through[1].GetInt32()] as Camera
                ?? throw new SceneLoadException($"{where}: renders through a camera that is not built yet");
            DisunityHost.RenderThroughCamera(canvas, camera, value.Float("plane_distance"));
        }
        else
        {
            // A camera canvas without a camera draws as an overlay, as in Unity.
            canvas.renderMode = mode == RenderMode.ScreenSpaceCamera ? RenderMode.ScreenSpaceOverlay : mode;
        }
        canvas.pixelPerfect = value.Bool("pixel_perfect");
        return canvas;
    }

    private static CanvasGroup CanvasGroup(GameObject owner, JsonElement value)
    {
        var group = owner.AddComponent<CanvasGroup>();
        group.alpha = value.Float("alpha");
        group.interactable = value.Bool("interactable");
        group.blocksRaycasts = value.Bool("blocks_raycasts");
        group.ignoreParentGroups = value.Bool("ignore_parent_groups");
        group.enabled = value.Bool("enabled");
        return group;
    }

    private static CanvasRenderer CanvasRenderer(GameObject owner, JsonElement value)
    {
        var renderer = owner.AddComponent<CanvasRenderer>();
        renderer.cullTransparentMesh = value.Bool("cull_transparent_mesh");
        return renderer;
    }

    /// <summary>Applies a CanvasScaler to the canvas on its object. The scaler has no engine
    /// object of its own.</summary>
    private object? Scale(GameObject owner, JsonElement value, string where)
    {
        var canvas = owner.GetComponent<Canvas>()
            ?? throw new SceneLoadException($"{where}: needs a Canvas earlier on its object");
        if (value.Float("reference_pixels_per_unit") != 100)
            throw new SceneLoadException($"{where}: only 100 reference pixels per unit are supported");
        if (!value.Bool("enabled")) return null;
        var scale = value.GetProperty("scale");
        if (scale.TryGetProperty("ConstantPixelSize", out var constant))
            canvas.scaleFactor = constant.Float("scale_factor");
        else
        {
            var screen = scale.GetProperty("ScaleWithScreenSize");
            if (screen.Str("screen_match") != "MatchWidthOrHeight")
                throw new SceneLoadException($"{where}: only the MatchWidthOrHeight screen match is supported");
            DisunityHost.ScaleWithScreenSize(canvas, screen.Vec2("reference_resolution"), screen.Float("match_width_or_height"));
        }
        return null;
    }

    private Text Text(GameObject owner, JsonElement value, string where)
    {
        var font = value.GetProperty("font");
        if (!value.Bool("enabled") || font.Bool("align_by_geometry") || font.Bool("best_fit"))
            throw new SceneLoadException($"{where}: disabled text, alignment by geometry and best fit are not supported");
        var text = DisunityHost.CreateText(owner, Font(font.Str("font")));
        text.text = value.Str("text");
        text.color = value.Rgba("color");
        text.fontSize = font.Int("size");
        text.fontStyle = font.Variant<FontStyle>("style");
        text.alignment = font.Variant<TextAnchor>("alignment");
        text.lineSpacing = font.Float("line_spacing");
        text.horizontalOverflow = font.Str("horizontal_overflow") == "Wrap" ? HorizontalWrapMode.Wrap : HorizontalWrapMode.Overflow;
        text.verticalOverflow = font.Variant<VerticalWrapMode>("vertical_overflow");
        text.supportRichText = font.Bool("rich_text");
        return text;
    }
}
