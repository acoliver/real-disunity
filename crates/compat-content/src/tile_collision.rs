use crate::{Diagnostic, ImportSidecar, ImportedComponent, ImportedScene, native, refs::reference};
use serde_json::{Value, json};

pub(crate) fn validate(sidecar: &ImportSidecar) -> Result<(), crate::ImportError> {
    let mut keys = std::collections::BTreeSet::new();
    for policy in &sidecar.tile_collider_fallbacks {
        if policy.reason.trim().is_empty()
            || policy.guid.trim().is_empty()
            || !keys.insert((&policy.guid, policy.file_id))
        {
            return Err(crate::ImportError::Source {
                path: "sidecar.tile_collider_fallbacks".into(),
                message: "sprite_bounds requires a unique source GUID/file ID and nonempty geometry fidelity reason".into(),
            });
        }
    }
    Ok(())
}

pub(crate) fn prepare(scene: &mut ImportedScene, sidecar: &ImportSidecar) {
    let results: Vec<_> = scene
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == "TilemapCollider2D")
        .map(|(index, c)| (index, convert(c, scene, sidecar)))
        .collect();
    for (index, result) in results {
        let c = &mut scene.components[index];
        match result {
            Ok((fields, fallback_cells)) => {
                c.native_fields = fields;
                if fallback_cells > 0 {
                    let policy = fallback(c, sidecar).expect("selected bounds policy");
                    scene.diagnostics.push(diagnostic(c, "info", "tile_collision_geometry_fallback",
                        format!("{fallback_cells} cells use sprite_bounds geometry from source rect/PPU/pivot; no alpha tracing. {}", policy.reason)));
                }
            }
            Err(message) => {
                c.native_kind = None;
                c.native_fields = json!({});
                scene
                    .diagnostics
                    .push(diagnostic(c, "error", "invalid_tile_collider", message));
            }
        }
    }
}
fn diagnostic(c: &ImportedComponent, severity: &str, code: &str, message: String) -> Diagnostic {
    Diagnostic {
        severity: severity.into(),
        code: code.into(),
        path: String::new(),
        guid: c.source.guid.clone(),
        file_id: Some(c.source.file_id),
        component: Some(c.kind.clone()),
        field: Some("m_Tiles".into()),
        message,
    }
}
fn fallback<'a>(
    c: &ImportedComponent,
    sidecar: &'a ImportSidecar,
) -> Option<&'a crate::TileColliderFallback> {
    sidecar
        .tile_collider_fallbacks
        .iter()
        .find(|p| p.guid == c.source.guid && p.file_id == c.source.file_id)
}
fn convert(
    c: &ImportedComponent,
    scene: &ImportedScene,
    sidecar: &ImportSidecar,
) -> Result<(Value, usize), String> {
    validate_source(c)?;
    let tilemap = scene
        .components
        .iter()
        .find(|t| t.kind == "Tilemap" && t.object == c.object)
        .filter(|t| t.native_kind.is_some())
        .ok_or("collider requires a valid Tilemap on the same owner")?;
    let fields = &tilemap.native_fields;
    let grid = &fields["grid"];
    for key in ["m_CellLayout", "m_CellSwizzle"] {
        if grid[key].as_i64().unwrap_or(0) != 0 {
            return Err(format!("unsupported planar rectangular grid {key}"));
        }
    }
    let size = native::vector(&grid["m_CellSize"], ["x", "y"], [1.; 2]);
    let gap = native::vector(&grid["m_CellGap"], ["x", "y"], [0.; 2]);
    if size.iter().zip(gap).any(|(s, g)| *s <= 0. || *s + g <= 0.) {
        return Err("invalid tile grid size or spacing".into());
    }
    let tiles = fields["tiles"]
        .as_array()
        .ok_or("normalized Tilemap cells absent")?;
    if tiles.len() > 4096 {
        return Err("tile collider cell budget exceeds 4096".into());
    }
    let mut cells = Vec::new();
    let mut fallback_cells = 0;
    let mut shapes = 0;
    for tile in tiles {
        let (cell, bounds) = cell(tile, scene, fallback(c, sidecar).is_some())?;
        fallback_cells += usize::from(bounds);
        shapes += match cell["collider_type"].as_str() {
            Some("none") => 0,
            _ => cell["physics_shapes"]
                .as_array()
                .map_or(1, |s| s.len().max(1)),
        };
        if shapes > 4096 {
            return Err("tile collider shape budget exceeds 4096".into());
        }
        cells.push(cell);
    }
    let mut result = c.native_fields.clone();
    result["cell_size"] = json!(size);
    result["cell_gap"] = json!(gap);
    result["tile_anchor"] = json!([fields["tile_anchor"][0], fields["tile_anchor"][1]]);
    result["cells"] = json!(cells);
    Ok((result, fallback_cells))
}
fn validate_source(c: &ImportedComponent) -> Result<(), String> {
    for key in [
        "m_CompositeOperation",
        "m_UsedByComposite",
        "m_UsedByEffector",
        "m_ExtrusionFactor",
    ] {
        if c.fields[key].as_f64().unwrap_or(0.) != 0. {
            return Err(format!("unsupported tile collision source option {key}"));
        }
    }
    if !c.fields["m_Material"].is_null() && c.native_fields["material"].is_null() {
        return Err(
            "tile collider material reference did not resolve to source coefficients".into(),
        );
    }
    Ok(())
}
fn cell(
    tile: &Value,
    scene: &ImportedScene,
    bounds_allowed: bool,
) -> Result<(Value, bool), String> {
    let coordinates = tile["position"].as_array().ok_or("cell position absent")?;
    if coordinates.len() != 3 || coordinates[2].as_i64() != Some(0) {
        return Err("cell position must be planar with integer z=0".into());
    }
    let position: Result<Vec<_>, _> = coordinates[..2]
        .iter()
        .map(|v| {
            v.as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or("cell position must fit signed 32-bit integers")
        })
        .collect();
    let tile_id = reference(&tile["tile"]).ok_or("tile asset reference absent")?;
    let document = scene
        .documents
        .iter()
        .find(|d| d.source.key() == tile_id)
        .ok_or_else(|| format!("tile asset {tile_id} absent"))?;
    let kind = match document.fields["m_ColliderType"].as_i64() {
        Some(0) => "none",
        Some(1) => "sprite",
        Some(2) => "grid",
        _ => return Err("unsupported or absent tile m_ColliderType".into()),
    };
    let mut result = json!({"position":position?, "collider_type":kind,
        "matrix":affine(&tile["matrix"])?, "sprite_size":null,
        "sprite_pivot":[0.5,0.5], "physics_shapes":[]});
    if kind != "sprite" {
        return Ok((result, false));
    }
    let (sprite, metadata) = sprite(&tile["sprite"], scene)?;
    let size = [
        sprite.rect[2] / sprite.pixels_per_unit,
        sprite.rect[3] / sprite.pixels_per_unit,
    ];
    if !sprite.pixels_per_unit.is_finite()
        || sprite.pixels_per_unit <= 0.
        || size.iter().any(|v| !v.is_finite() || *v <= 0.)
        || sprite.pivot.iter().any(|v| !(0. ..=1.).contains(v))
    {
        return Err("invalid source sprite rect/PPU/pivot".into());
    }
    let shapes =
        crate::tile_geometry::physics_shapes(&metadata["physicsShape"], sprite.pixels_per_unit)?;
    let bounds = shapes.is_empty();
    if bounds && !bounds_allowed {
        return Err(
            "sprite physicsShape is empty; explicit sprite_bounds sidecar selection required"
                .into(),
        );
    }
    result["sprite_size"] = json!(size);
    result["sprite_pivot"] = json!(sprite.pivot);
    result["physics_shapes"] = json!(shapes);
    Ok((result, bounds))
}
fn sprite<'a>(
    value: &Value,
    scene: &'a ImportedScene,
) -> Result<(&'a crate::SpriteAsset, &'a Value), String> {
    let asset = scene
        .assets
        .iter()
        .find(|a| Some(a.guid.as_str()) == value["guid"].as_str())
        .ok_or("tile sprite asset absent")?;
    let sprite = asset
        .sprites
        .iter()
        .find(|s| Some(s.file_id) == value["file_id"].as_i64())
        .ok_or("tile sprite subasset absent")?;
    let importer = &asset.metadata["TextureImporter"];
    let sheet = &importer["spriteSheet"];
    if importer["spriteMode"].as_i64() == Some(1) {
        return Ok((sprite, sheet));
    }
    let metadata = sheet["sprites"]
        .as_array()
        .and_then(|sprites| {
            sprites.iter().find(|s| {
                s["internalID"].as_i64() == Some(sprite.file_id)
                    || s["name"].as_str() == Some(sprite.name.as_str())
            })
        })
        .ok_or("source sprite physics metadata absent")?;
    Ok((sprite, metadata))
}
fn affine(value: &Value) -> Result<[f64; 6], String> {
    if value.is_null() {
        return Ok([1., 0., 0., 1., 0., 0.]);
    }
    let mut matrix = [[0.; 4]; 4];
    for (row, values) in matrix.iter_mut().enumerate() {
        for (column, entry) in values.iter_mut().enumerate() {
            *entry = value[format!("e{row}{column}")]
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or("tile matrix requires all 16 finite entries")?;
        }
    }
    for (row, column, expected) in [
        (0, 2, 0.),
        (1, 2, 0.),
        (2, 0, 0.),
        (2, 1, 0.),
        (2, 2, 1.),
        (2, 3, 0.),
        (3, 0, 0.),
        (3, 1, 0.),
        (3, 2, 0.),
        (3, 3, 1.),
    ] {
        if (matrix[row][column] - expected).abs() > 0. {
            return Err("nonplanar or perspective tile matrix is unsupported".into());
        }
    }
    let [xx, xy, _, tx] = matrix[0];
    let [yx, yy, _, ty] = matrix[1];
    if (xx * yy - xy * yx).abs() < 1e-8 {
        return Err("singular tile affine matrix".into());
    }
    Ok([xx, yx, xy, yy, tx, ty])
}
