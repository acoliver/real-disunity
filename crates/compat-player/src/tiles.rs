use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};

pub(crate) fn sprites(object: &Value, component: &Value) -> Result<Vec<(Value, Value)>, String> {
    let fields = &component["fields"];
    let renderer = &fields["renderer"];
    if renderer["m_Enabled"] == 0 {
        return Ok(Vec::new());
    }
    let grid = &fields["grid"];
    if grid["m_CellLayout"].as_u64().unwrap_or(0) != 0
        || grid["m_CellSwizzle"].as_u64().unwrap_or(0) != 0
    {
        return Err("tile extraction requires rectangular XYZ grid layout".into());
    }
    let cells = fields["tiles"]
        .as_array()
        .ok_or("Tilemap has no normalized tile array")?;
    if cells.len() > 65536 {
        return Err("tile contribution budget exceeded".into());
    }
    let pose = &object["transform"];
    let world = Mat4::from_scale_rotation_translation(
        vector(&pose["scale"])?,
        quaternion(&pose["rotation"])?,
        vector(&pose["position"])?,
    );
    let cell = named_vector(&grid["m_CellSize"], Vec3::ONE)?;
    let gap = named_vector(&grid["m_CellGap"], Vec3::ZERO)?;
    let anchor = vector(&fields["tile_anchor"])?;
    let orientation = matrix(&fields["m_TileOrientationMatrix"])?;
    cells.iter().filter(|tile|!tile["sprite"].is_null()).map(|tile|{
        let position=vector(&tile["position"])?*(cell+gap)+anchor*cell;
        let transform=world*Mat4::from_translation(position)*orientation*matrix(&tile["matrix"])?;
        let (scale,rotation,position)=decompose(transform)?;
        let mut tint=serde_json::from_value::<[f32;4]>(fields["color"].clone()).map_err(|error|error.to_string())?;
        let tile_tint=&tile["color"];
        for (index,key) in ["r","g","b","a"].iter().enumerate() {
            tint[index]*=serde_json::from_value::<f32>(tile_tint[*key].clone()).map_err(|error|error.to_string())?;
        }
        Ok((json!({"transform":{"position":position.to_array(),"rotation":rotation.to_array(),"scale":scale.to_array()}}),
            json!({"fields":{"sprite":tile["sprite"],"color":tint,"flip_x":false,"flip_y":false,
                "sorting_layer":renderer["m_SortingLayer"].as_i64().unwrap_or(0),
                "sorting_order":renderer["m_SortingOrder"].as_i64().unwrap_or(0)}})))
    }).collect()
}
fn vector(value: &Value) -> Result<Vec3, String> {
    serde_json::from_value::<[f32; 3]>(value.clone())
        .map(Vec3::from_array)
        .map_err(|error| error.to_string())
}
fn quaternion(value: &Value) -> Result<Quat, String> {
    serde_json::from_value::<[f32; 4]>(value.clone())
        .map(|value| Quat::from_array(value).normalize())
        .map_err(|error| error.to_string())
}
fn named_vector(value: &Value, default: Vec3) -> Result<Vec3, String> {
    if value.is_null() {
        return Ok(default);
    }
    Ok(Vec3::new(
        number(&value["x"])?,
        number(&value["y"])?,
        number(&value["z"])?,
    ))
}
fn number(value: &Value) -> Result<f32, String> {
    serde_json::from_value(value.clone()).map_err(|error| error.to_string())
}
fn matrix(value: &Value) -> Result<Mat4, String> {
    if value.is_null() {
        return Ok(Mat4::IDENTITY);
    }
    let mut elements = [0.; 16];
    for column in 0..4 {
        for row in 0..4 {
            elements[column * 4 + row] = number(&value[format!("e{row}{column}")])?;
        }
    }
    let result = Mat4::from_cols_array(&elements);
    if !result.is_finite() {
        return Err("nonfinite tile matrix".into());
    }
    Ok(result)
}
fn decompose(matrix: Mat4) -> Result<(Vec3, Quat, Vec3), String> {
    let (scale, rotation, position) = matrix.to_scale_rotation_translation();
    if !scale.is_finite()
        || scale.abs().min_element() < 1e-8
        || !rotation.is_finite()
        || !position.is_finite()
    {
        return Err("tile transform is singular or nonfinite".into());
    }
    let rebuilt = Mat4::from_scale_rotation_translation(scale, rotation, position);
    if !matrix.abs_diff_eq(rebuilt, 1e-4) {
        return Err("tile transform shear cannot be represented by shared dynamic quads".into());
    }
    Ok((scale, rotation, position))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    #[test]
    fn actual_tiles_use_authored_cell_spacing_anchor_sprite_and_color() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let crate_root = root.join("crates/compat-content");
        let profile = serde_json::from_slice(
            &std::fs::read(crate_root.join("profiles/breakout-source.json")).unwrap(),
        )
        .unwrap();
        let sidecar = serde_json::from_slice(
            &std::fs::read(crate_root.join("overlays/breakout.json")).unwrap(),
        )
        .unwrap();
        let graph = worldrunner::compat_content::import_project(
            &worldrunner_original_games::checkout(worldrunner_original_games::Game::Breakout)
                .unwrap(),
            Path::new("Assets/Scenes/SampleScene.unity"),
            &profile,
            &sidecar,
        )
        .unwrap();
        let graph = serde_json::to_value(graph).unwrap();
        let component = graph["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["native_kind"] == "Tilemap")
            .unwrap();
        let source = json!({"transform":{"position":[3.,4.,0.],"rotation":[0.,0.,0.,1.],"scale":[1.,1.,1.]}});
        let native = json!({"fields":component["native_fields"]});
        let sprites = sprites(&source, &native).unwrap();
        assert_eq!(sprites.len(), 48);
        assert_eq!(
            sprites[0].0["transform"]["position"],
            json!([-6., -1.5, 0.])
        );
        assert_eq!(
            sprites[0].1["fields"]["sprite"]["file_id"],
            -2_105_052_758_i64
        );
        assert_eq!(sprites[0].1["fields"]["color"], json!([1., 1., 1., 1.]));
    }
}
