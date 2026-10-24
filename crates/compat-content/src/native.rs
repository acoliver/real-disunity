use serde_json::{Value, json};

pub(crate) fn number(v: &Value, fallback: f64) -> f64 {
    v.as_f64().unwrap_or(fallback)
}
pub(crate) fn boolean(v: &Value, fallback: bool) -> bool {
    v.as_bool()
        .or_else(|| v.as_i64().map(|v| v != 0))
        .unwrap_or(fallback)
}
pub(crate) fn vector<const N: usize>(v: &Value, keys: [&str; N], fallback: [f64; N]) -> [f64; N] {
    std::array::from_fn(|i| number(&v[keys[i]], fallback[i]))
}
fn xy(v: &Value, fallback: [f64; 2]) -> [f64; 2] {
    vector(v, ["x", "y"], fallback)
}
fn rgba(v: &Value, fallback: [f64; 4]) -> [f64; 4] {
    vector(v, ["r", "g", "b", "a"], fallback)
}

fn polygon(fields: &Value) -> Value {
    let points = fields["m_Points"]["m_Paths"]
        .get(0)
        .and_then(Value::as_array)
        .map(|v| v.iter().map(|p| xy(p, [0.; 2])).collect::<Vec<_>>())
        .unwrap_or_default();
    json!({"points":points,
        "offset":xy(&fields["m_Offset"], [0.;2]),
        "is_trigger":boolean(&fields["m_IsTrigger"], false)})
}

pub(crate) fn convert(
    kind: &str,
    fields: &Value,
    script_type: Option<&str>,
) -> (Option<String>, Value) {
    if let Some((kind, data)) = crate::ui_native::convert(kind, fields, script_type) {
        return (Some(kind.into()), data);
    }
    let (native, data) = match kind {
        "Transform" => ("Transform", json!({})),
        "Rigidbody2D" => (
            "Rigidbody2D",
            json!({
                "velocity": xy(fields.get("m_LinearVelocity").unwrap_or(&fields["m_Velocity"]), [0.;2]),
                "body_type": match fields["m_BodyType"].as_i64().unwrap_or(0) { 1 => "kinematic", 2 => "static", _ => "dynamic" },
                "gravity_scale": number(&fields["m_GravityScale"], 1.), "mass": number(&fields["m_Mass"], 1.),
                "drag": number(fields.get("m_LinearDamping").unwrap_or(&fields["m_LinearDrag"]), 0.),
                "angular_velocity":number(&fields["m_AngularVelocity"],0.),
                "angular_drag":number(fields.get("m_AngularDamping").unwrap_or(&fields["m_AngularDrag"]),0.),
                "constraints":fields["m_Constraints"].as_u64().unwrap_or(0),
                "simulated": boolean(&fields["m_Simulated"], true),
            }),
        ),
        "BoxCollider2D" => (
            "BoxCollider2D",
            json!({ "size": xy(&fields["m_Size"], [1.;2]), "offset": xy(&fields["m_Offset"], [0.;2]), "is_trigger": boolean(&fields["m_IsTrigger"], false) }),
        ),
        "CircleCollider2D" => (
            "CircleCollider2D",
            json!({ "radius": number(&fields["m_Radius"], 0.5), "offset": xy(&fields["m_Offset"], [0.;2]), "is_trigger": boolean(&fields["m_IsTrigger"], false) }),
        ),
        "PolygonCollider2D" => ("PolygonCollider2D", polygon(fields)),
        "SpriteRenderer" => (
            "SpriteRenderer",
            json!({
                "sprite": fields["m_Sprite"], "color": rgba(&fields["m_Color"], [1.;4]), "size": xy(&fields["m_Size"], [1.;2]),
                "flip_x":boolean(&fields["m_FlipX"], false), "flip_y":boolean(&fields["m_FlipY"], false),
                "sorting_layer":fields["m_SortingLayerID"].as_i64().unwrap_or(0), "sorting_order":fields["m_SortingOrder"].as_i64().unwrap_or(0),
            }),
        ),
        "Camera" => (
            "Camera",
            json!({
                "orthographic":boolean(&fields["orthographic"], false), "orthographic_size":number(&fields["orthographic size"], 5.),
                "background":rgba(&fields["m_BackGroundColor"], [0.,0.,0.,1.]), "near":number(&fields["near clip plane"], 0.3),
                "far":number(&fields["far clip plane"], 1000.), "depth":number(&fields["m_Depth"], 0.),
            }),
        ),
        "TilemapCollider2D" => (
            "TilemapCollider2D",
            json!({"offset":xy(&fields["m_Offset"], [0.;2]),
                "is_trigger":boolean(&fields["m_IsTrigger"], false), "material":null}),
        ),
        "Tilemap" => ("Tilemap", fields.clone()),
        "MonoBehaviour" if script_type.is_some() => ("Script", json!({"type":script_type})),
        _ => return (None, json!({})),
    };
    (Some(native.into()), data)
}
