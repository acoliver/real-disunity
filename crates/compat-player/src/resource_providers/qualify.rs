use super::{ResourceProviders, binding::Binding, catalog, gpu, native};
use crate::extraction::{AssetTable, Contribution};
use serde_json::{Value, json};
use std::path::Path;
use worldrunner::{
    compat_content::ImportedScene,
    managed::CompatibilityHost,
    scene::gpu::{Viewport, device::GpuContext},
};

pub(super) fn run<'gpu>(
    root: &Path,
    context: &'gpu GpuContext,
    control: gpu::Control,
) -> Result<ResourceProviders<'gpu>, String> {
    let binding = Binding::load(root)?;
    let assets = assets(binding.root())?;
    let mut renderer = gpu::Renderer::new(context, &assets, control)?;
    let mut host = CompatibilityHost::boot(&binding.config()).map_err(|e| e.to_string())?;
    let world = host.create_world().map_err(|e| e.to_string())?;
    // The released facade retains the native image across host teardown and TLS cleanup.
    // This empty fixture binds that facade without running managed script callbacks.
    host.load_fixture(&json!({"world":world,
        "assemblies":[binding.fixture_assembly()],
        "graph":{"nodes":[],"components":[],"assets":[],"aliases":{}},"handles":{}
    }))
    .map_err(|e| e.to_string())?;
    binding.verify_mapped()?;
    let fixture = native::Fixture::open(&host, world)?;
    let captures = contracts(&host, world, &fixture, &assets, &mut renderer);
    let finish = fixture.finish(&host);
    let captures = match captures.and_then(|captures| finish.map(|()| captures)) {
        Ok(captures) => captures,
        Err(error) => {
            let _ = host.destroy_world(world);
            return Err(error);
        }
    };
    binding.verify()?;
    let evidence = serde_json::to_vec(&json!({
        "contract":"exact-canonical-resources-v1", "files":binding.files(),
        "image_sha256":binding.image_sha256(), "captures":captures,
        "adapter":format!("{:?}",context.adapter().get_info()),
        "pipeline":"actual native snapshot/AssetTable/DynamicQuad/SceneGpu/PostChain/CopyOutput/Readback",
        "semantics":"linear tint, straight source-alpha blending, less-equal depth test, no depth writes; stable ascending layer/order",
        "losses":["original Unity shader execution, URP, lighting and material properties", "original missing Snake artwork"],
        "unsupported":["MappedFont","MappedTileType","game activation","arbitrary materials, shaders and images"],
        "sprite_scope":"exact authored symmetric white 8x8 unit square; UV flips are visually invariant; no arbitrary atlas/orientation fidelity",
        "native_scope":"zero-frame phase-owned setters; no managed gameplay or original game PE behavior"
    })).map_err(|e| e.to_string())?;
    let providers = catalog::providers(&binding, &evidence);
    Ok(ResourceProviders {
        host,
        world,
        binding,
        assets,
        renderer,
        providers,
        evidence,
    })
}
fn assets(root: &Path) -> Result<AssetTable, String> {
    let graph: ImportedScene = serde_json::from_value(json!({
        "scene":"authored-resource-fixture","scene_guid":"authored-resource-fixture",
        "nodes":[],"components":[],"assets":[],"documents":[],"aliases":{},
        "diagnostics":[],"source_hashes":{},"fallback_sprites":[catalog::sprite()?]
    }))
    .map_err(|e| e.to_string())?;
    AssetTable::load(root, &graph)
}
fn contribution(
    host: &CompatibilityHost,
    world: u64,
    assets: &AssetTable,
) -> Result<(Value, Contribution), String> {
    let snapshot = native::request(host, world, &json!({"op":"snapshot"}))?;
    let contribution = assets.extract(
        &snapshot,
        Viewport {
            width: 64,
            height: 64,
        },
    )?;
    Ok((snapshot, contribution))
}
pub(super) fn capture(
    host: &CompatibilityHost,
    world: u64,
    assets: &AssetTable,
    renderer: &mut gpu::Renderer<'_>,
    material: &str,
) -> Result<Vec<u8>, String> {
    let (_, mut contribution) = contribution(host, world, assets)?;
    catalog::apply(catalog::material(material)?, &mut contribution.quads)?;
    renderer.capture(&contribution)
}
fn record(
    host: &CompatibilityHost,
    world: u64,
    assets: &AssetTable,
    renderer: &mut gpu::Renderer<'_>,
    material: &str,
    name: &str,
) -> Result<Value, String> {
    let (snapshot, mut contribution) = contribution(host, world, assets)?;
    catalog::apply(catalog::material(material)?, &mut contribution.quads)?;
    let pixels = renderer.capture(&contribution)?;
    Ok(json!({"name":name,"material":material,"snapshot":snapshot,"rgba8":pixels}))
}
fn pixels(record: &Value) -> Result<Vec<u8>, String> {
    serde_json::from_value(record["rgba8"].clone()).map_err(|e| e.to_string())
}
fn pixel(record: &Value, x: usize, y: usize) -> Result<[u8; 4], String> {
    let pixels = pixels(record)?;
    let offset = (y * 64 + x) * 4;
    pixels[offset..offset + 4]
        .try_into()
        .map_err(|_| "invalid GPU readback".into())
}
pub(super) fn exact_pixel(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual == expected
}
fn require(value: bool, contract: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!(
            "resource behavior qualification failed: {contract}"
        ))
    }
}
fn contracts(
    host: &CompatibilityHost,
    world: u64,
    fixture: &native::Fixture,
    assets: &AssetTable,
    renderer: &mut gpu::Renderer<'_>,
) -> Result<Vec<Value>, String> {
    let white = record(
        host,
        world,
        assets,
        renderer,
        catalog::WHITE,
        "authored-rect-pivot-ppu",
    )?;
    require(
        pixel(&white, 32, 32)? == [255; 4]
            && pixel(&white, 24, 24)? == [255; 4]
            && pixel(&white, 23, 24)? == [0, 0, 0, 255]
            && pixel(&white, 40, 32)? == [0, 0, 0, 255],
        "exact authored square geometry and bytes",
    )?;
    let green = record(
        host,
        world,
        assets,
        renderer,
        catalog::GREEN,
        "named-material-tint-alpha",
    )?;
    require(
        exact_pixel(pixel(&green, 32, 32)?, [0, 188, 0, 255]),
        &format!(
            "named material controls linear tint and alpha on canonical shared pipeline; actual={:?}",
            pixel(&green, 32, 32)?
        ),
    )?;
    fixture.mutate(host, &json!({"op":"component.set","component":fixture.component,"fields":{"color":[0.5,0.25,1.,0.5]}}))?;
    let tinted = record(
        host,
        world,
        assets,
        renderer,
        catalog::WHITE,
        "native-color-alpha",
    )?;
    require(
        exact_pixel(pixel(&tinted, 32, 32)?, [137, 99, 188, 255]),
        "native tint/alpha reaches canonical GPU",
    )?;
    fixture.mutate(host, &json!({"op":"transform.set","object":fixture.object,"position":[1.,0.,0.],"scale":[2.,1.,1.]}))?;
    let moved = record(host, world, assets, renderer, catalog::WHITE, "native-trs")?;
    require(
        pixel(&moved, 24, 32)? == [0, 0, 0, 255]
            && exact_pixel(pixel(&moved, 60, 32)?, [137, 99, 188, 255]),
        "connected native geometry",
    )?;
    let mut captures = vec![white, green, tinted, moved];
    orientation(host, world, fixture, assets, renderer, &mut captures)?;
    sorting(host, world, fixture, assets, renderer, &mut captures)?;
    Ok(captures)
}
fn orientation(
    host: &CompatibilityHost,
    world: u64,
    fixture: &native::Fixture,
    assets: &AssetTable,
    renderer: &mut gpu::Renderer<'_>,
    captures: &mut Vec<Value>,
) -> Result<(), String> {
    let half = 0.5_f64.sqrt();
    fixture.mutate(
        host,
        &json!({"op":"transform.set","object":fixture.object,"rotation":[0.,0.,half,half]}),
    )?;
    let rotated = record(
        host,
        world,
        assets,
        renderer,
        catalog::WHITE,
        "native-orientation",
    )?;
    require(
        pixel(&rotated, 60, 32)? == [0, 0, 0, 255]
            && exact_pixel(pixel(&rotated, 48, 44)?, [137, 99, 188, 255]),
        "native rotation of nonuniform scale",
    )?;
    fixture.mutate(host, &json!({"op":"component.set","component":fixture.component,"fields":{"flip_x":true,"flip_y":true}}))?;
    let flipped = record(
        host,
        world,
        assets,
        renderer,
        catalog::WHITE,
        "symmetric-uv-flip",
    )?;
    require(
        pixels(&rotated)? == pixels(&flipped)?,
        "exact solid sprite flip invariance",
    )?;
    fixture.mutate(
        host,
        &json!({"op":"component.set","component":fixture.component,"enabled":false}),
    )?;
    let hidden = record(
        host,
        world,
        assets,
        renderer,
        catalog::WHITE,
        "native-disabled",
    )?;
    require(
        pixels(&hidden)?
            .chunks_exact(4)
            .all(|p| p == [0, 0, 0, 255]),
        "native enabled controls geometry",
    )?;
    fixture.mutate(host, &json!({"op":"component.set","component":fixture.component,"enabled":true,"fields":{"color":[1.,1.,1.,1.],"flip_x":false,"flip_y":false}}))?;
    fixture.mutate(host, &json!({"op":"transform.set","object":fixture.object,"position":[0.,0.,0.],"scale":[1.,1.,1.],"rotation":[0.,0.,0.,1.]}))?;
    captures.extend([rotated, flipped, hidden]);
    Ok(())
}
fn sorting(
    host: &CompatibilityHost,
    world: u64,
    fixture: &native::Fixture,
    assets: &AssetTable,
    renderer: &mut gpu::Renderer<'_>,
    captures: &mut Vec<Value>,
) -> Result<(), String> {
    let object = fixture.mutate(
        host,
        &json!({"op":"object.create","name":"ordered-red-backdrop"}),
    )?["handle"]
        .clone();
    let component = fixture.mutate(host, &json!({"op":"component.add","object":object,"kind":"SpriteRenderer","fields":{"sprite":{"$builtin":catalog::SPRITE},"color":[1.,0.,0.,1.],"sorting_layer":0,"sorting_order":0}}))?["handle"].clone();
    let (snapshot, mut contribution) = contribution(host, world, assets)?;
    require(
        contribution.quads.len() == 2,
        "actual two native sprite contributions",
    )?;
    catalog::apply(
        catalog::material(catalog::GREEN)?,
        &mut contribution.quads[..1],
    )?;
    catalog::apply(
        catalog::material(catalog::WHITE)?,
        &mut contribution.quads[1..],
    )?;
    let rgba = renderer.capture(&contribution)?;
    let record = json!({"name":"material-order-no-depth-write","snapshot":snapshot,"rgba8":rgba});
    require(
        exact_pixel(pixel(&record, 32, 32)?, [188, 188, 0, 255]),
        "material sorting overrides native submission order with source-alpha and no depth writes",
    )?;
    fixture.mutate(
        host,
        &json!({"op":"component.set","component":component,"enabled":false}),
    )?;
    captures.push(record);
    Ok(())
}

#[cfg(test)]
#[path = "exact_native_tests.rs"]
mod exact_native_tests;
