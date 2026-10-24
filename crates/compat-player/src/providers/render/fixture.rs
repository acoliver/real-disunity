use super::super::{binding::Binding, input::request};
use serde_json::{Value, json};
use worldrunner::compat_content::{AssetKind, ImportedAsset, ImportedScene, SpriteAsset};
use worldrunner::managed::CompatibilityHost;

pub fn graph(binding: &Binding) -> Result<ImportedScene, String> {
    let mut graph:ImportedScene=serde_json::from_value(json!({"scene":"provider-microfixture","scene_guid":"independently-authored","nodes":[],"components":[],"assets":[],"documents":[],"aliases":{},"diagnostics":[],"source_hashes":{}})).map_err(|e|e.to_string())?;
    let path = ".work/resumed-provider-qualification-v2/atlas.png";
    let bytes = std::fs::read(binding.path(path)).map_err(|e| e.to_string())?;
    let hash = worldrunner::ContentHash::of(&bytes);
    graph.assets.push(ImportedAsset {
        guid: "qualification-atlas".into(),
        logical_id: worldrunner::LogicalAssetId::new("qualification.atlas")
            .map_err(|e| e.to_string())?,
        path: path.into(),
        kind: AssetKind::Texture,
        content_hash: hash,
        sha256: hash.to_hex(),
        byte_length: bytes.len() as u64,
        metadata: json!({}),
        texture_size: Some([4, 4]),
        sprites: vec![SpriteAsset {
            file_id: 1,
            name: "right-half".into(),
            rect: [2., 0., 2., 4.],
            pivot: [0.25, 0.5],
            pixels_per_unit: 2.,
            border: [0.; 4],
        }],
    });
    Ok(graph)
}
fn add(
    host: &CompatibilityHost,
    world: u64,
    object: &Value,
    kind: &str,
    fields: impl Into<Value>,
) -> Result<Value, String> {
    let fields = fields.into();
    Ok(request(
        host,
        world,
        json!({"op":"component.add","object":object,"kind":kind,"fields":fields}),
    )?["handle"]
        .clone())
}
pub fn world(host: &CompatibilityHost, world: u64) -> Result<(Value, Value), String> {
    let camera = request(
        host,
        world,
        json!({"op":"object.create","name":"canonical-camera"}),
    )?["handle"]
        .clone();
    request(
        host,
        world,
        json!({"op":"transform.set","object":camera,"position":[0.,0.,-10.]}),
    )?;
    add(
        host,
        world,
        &camera,
        "Camera",
        json!({"orthographic":true,"orthographic_size":2.,"near":0.1,"far":100.,"background":[0.,0.,0.,1.]}),
    )?;
    let sprite = request(
        host,
        world,
        json!({"op":"object.create","name":"canonical-sprite"}),
    )?["handle"]
        .clone();
    let component = add(
        host,
        world,
        &sprite,
        "SpriteRenderer",
        json!({"sprite":{"guid":"qualification-atlas","file_id":1},"color":[1.,1.,1.,1.],"sorting_layer":0,"sorting_order":0}),
    )?;
    let script = add(
        host,
        world,
        &sprite,
        "Script",
        json!({"type":"ProviderSpriteProbe"}),
    )?;
    let _ = component;
    tile_and_order(host, world)?;
    Ok((sprite, script))
}
fn tile_and_order(host: &CompatibilityHost, world: u64) -> Result<(), String> {
    let tile = request(
        host,
        world,
        json!({"op":"object.create","name":"canonical-tiles"}),
    )?["handle"]
        .clone();
    request(
        host,
        world,
        json!({"op":"transform.set","object":tile,"position":[-1.,0.,0.]}),
    )?;
    add(
        host,
        world,
        &tile,
        "Tilemap",
        json!({"grid":{},"tile_anchor":[0.,0.,0.],"color":[1.,1.,1.,1.],"renderer":{"m_Enabled":1,"m_SortingLayer":0,"m_SortingOrder":0},"tiles":[{"position":[0.,0.,0.],"sprite":{"guid":"qualification-atlas","file_id":1},"color":{"r":1.,"g":1.,"b":1.,"a":1.}}]}),
    )?;
    let overlay = request(
        host,
        world,
        json!({"op":"object.create","name":"ordering-overlay"}),
    )?["handle"]
        .clone();
    request(
        host,
        world,
        json!({"op":"transform.set","object":overlay,"position":[-1.,0.,0.]}),
    )?;
    add(
        host,
        world,
        &overlay,
        "SpriteRenderer",
        json!({"sprite":{"guid":"qualification-atlas","file_id":1},"color":[0.,0.,0.,0.5],"sorting_layer":0,"sorting_order":10}),
    )?;
    Ok(())
}
