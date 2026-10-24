use crate::native::{boolean, number, vector};
use serde_json::{Value, json};

pub(crate) fn convert(
    kind: &str,
    fields: &Value,
    script_type: Option<&str>,
) -> Option<(&'static str, Value)> {
    match (kind, script_type) {
        ("RectTransform", _) => Some((
            "RectTransform",
            json!({
                "anchor_min":vector(&fields["m_AnchorMin"], ["x","y"], [0.5;2]),
                "anchor_max":vector(&fields["m_AnchorMax"], ["x","y"], [0.5;2]),
                "pivot":vector(&fields["m_Pivot"], ["x","y"], [0.5;2]),
                "size_delta":vector(&fields["m_SizeDelta"], ["x","y"], [100.;2]),
                "anchored_position":vector(&fields["m_AnchoredPosition"], ["x","y"], [0.;2])
            }),
        )),
        ("Canvas", _) => Some((
            "Canvas",
            json!({
                "render_mode":match fields["m_RenderMode"].as_i64().unwrap_or(0) {
                    1 => "screen_space_camera", 2 => "world_space", _ => "screen_space_overlay"
                },
                "camera":fields["m_Camera"].get("$ref").map(|id| json!({"$ref":id})),
                "plane_distance":number(&fields["m_PlaneDistance"],100.),
                "sorting_layer":fields["m_SortingLayerID"].as_i64().unwrap_or(0),
                "sorting_order":fields["m_SortingOrder"].as_i64().unwrap_or(0)
            }),
        )),
        ("MonoBehaviour", Some("UnityEngine.UI.CanvasScaler")) => Some((
            "CanvasScaler",
            json!({
                "scale_mode":if fields["m_UiScaleMode"].as_i64() == Some(1) {"scale_with_screen_size"} else {"constant_pixel_size"},
                "scale_factor":number(&fields["m_ScaleFactor"],1.),
                "reference_resolution":vector(&fields["m_ReferenceResolution"], ["x","y"], [800.,600.]),
                "match_width_or_height":number(&fields["m_MatchWidthOrHeight"],0.)
            }),
        )),
        ("MonoBehaviour", Some("UnityEngine.UI.Text")) => Some(("Text", text(fields))),
        _ => None,
    }
}
fn text(fields: &Value) -> Value {
    let font = &fields["m_FontData"];
    let identity = &font["m_Font"];
    let name = identity["$builtin"]
        .as_str()
        .or_else(|| identity["$ref"].as_str())
        .unwrap_or("unresolved-font");
    let text = fields["m_Text"]
        .as_str()
        .map_or_else(|| fields["m_Text"].to_string(), str::to_owned);
    json!({
        "text":text,"font":name,"font_size":number(&font["m_FontSize"],14.),
        "color":vector(&fields["m_Color"],["r","g","b","a"],[1.;4]),
        "alignment":font["m_Alignment"].as_i64().unwrap_or(0),
        "line_spacing":number(&font["m_LineSpacing"],1.),
        "wrap":!boolean(&font["m_HorizontalOverflow"],false),
        "overflow":boolean(&font["m_VerticalOverflow"],false)
    })
}
