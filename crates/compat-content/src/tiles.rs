use crate::{Diagnostic, ImportedComponent, SpawnNode, native};
use serde_json::{Value, json};

pub(crate) fn prepare(
    components: &mut [ImportedComponent],
    nodes: &[SpawnNode],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let source = components.to_vec();
    for c in components.iter_mut().filter(|c| c.kind == "Tilemap") {
        match cells(&c.fields) {
            Ok(tiles) => {
                c.native_fields["tiles"] = tiles;
                c.native_fields["tile_anchor"] = json!(native::vector(
                    &c.fields["m_TileAnchor"],
                    ["x", "y", "z"],
                    [0.5, 0.5, 0.]
                ));
                c.native_fields["color"] = json!(native::vector(
                    &c.fields["m_Color"],
                    ["r", "g", "b", "a"],
                    [1.; 4]
                ));
                c.native_fields["renderer"] = source
                    .iter()
                    .find(|s| s.kind == "TilemapRenderer" && s.object == c.object)
                    .map_or(Value::Null, |s| s.fields.clone());
                c.native_fields["grid"] = grid(c, &source, nodes);
            }
            Err(message) => {
                c.native_kind = None;
                diagnostics.push(Diagnostic {
                    severity: "error".into(),
                    code: "invalid_tilemap".into(),
                    path: String::new(),
                    guid: c.source.guid.clone(),
                    file_id: Some(c.source.file_id),
                    component: Some(c.kind.clone()),
                    field: Some("m_Tiles".into()),
                    message,
                });
            }
        }
    }
}
fn grid(
    component: &ImportedComponent,
    components: &[ImportedComponent],
    nodes: &[SpawnNode],
) -> Value {
    let mut owner = component.object.as_ref();
    for _ in 0..=nodes.len() {
        let Some(id) = owner else {
            break;
        };
        if let Some(grid) = components
            .iter()
            .find(|c| c.kind == "Grid" && c.object.as_ref() == Some(id))
        {
            return grid.fields.clone();
        }
        owner = nodes
            .iter()
            .find(|n| &n.id == id)
            .and_then(|n| n.parent.as_ref());
    }
    Value::Null
}
fn cells(fields: &Value) -> Result<Value, String> {
    let Some(tiles) = fields["m_Tiles"].as_array() else {
        if fields["m_Tiles"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        {
            return Ok(json!([]));
        }
        return Err("m_Tiles must contain an array of authored cells".into());
    };
    tiles.iter().map(|tile| {
        let position: Result<Vec<_>, _> = ["x","y","z"].iter().map(|key|
            tile["first"][key].as_i64().ok_or_else(|| format!("tile coordinate {key} must be an integer"))).collect();
        let values = &tile["second"];
        Ok(json!({
            "position":position?, "tile":entry(fields, values, "m_TileAssetArray", "m_TileIndex")?,
            "sprite":entry(fields, values, "m_TileSpriteArray", "m_TileSpriteIndex")?,
            "color":entry(fields, values, "m_TileColorArray", "m_TileColorIndex")?,
            "matrix":entry(fields, values, "m_TileMatrixArray", "m_TileMatrixIndex")?,
            "object":entry(fields, values, "m_TileObjectToInstantiateArray", "m_TileObjectToInstantiateIndex")?,
            "flags":values["m_AllTileFlags"]
        }))
    }).collect::<Result<Vec<_>, String>>().map(Value::Array)
}
fn entry(fields: &Value, tile: &Value, table: &str, index: &str) -> Result<Value, String> {
    let id = tile[index]
        .as_u64()
        .ok_or_else(|| format!("tile index {index} must be unsigned"))?;
    if id == 65_535 || id == u64::from(u32::MAX) {
        return Ok(Value::Null);
    }
    let id = usize::try_from(id).map_err(|e| e.to_string())?;
    fields[table]
        .get(id)
        .and_then(|value| value.get("m_Data"))
        .cloned()
        .ok_or_else(|| format!("tile index {index}={id} is absent in {table}"))
}
