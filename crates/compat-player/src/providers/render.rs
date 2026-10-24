#[path = "render/fixture.rs"]
mod fixture;
#[path = "render/gpu.rs"]
mod gpu;
#[path = "render/stage.rs"]
mod stage;
use super::{binding::Binding, input::require};
use crate::extraction::AssetTable;
use serde_json::{Value, json};
use worldrunner::{
    managed::CompatibilityHost,
    scene::gpu::{Viewport, device::GpuContext},
};

pub(crate) fn qualify(
    host: &mut CompatibilityHost,
    binding: &Binding,
    context: &GpuContext,
    control: super::RenderControl,
) -> Result<Value, String> {
    let world = host.create_world().map_err(|e| e.to_string())?;
    let result = run(host, binding, context, control, world);
    let unload = host.unload().map_err(|e| e.to_string());
    let destroy = host.destroy_world(world).map_err(|e| e.to_string());
    let evidence = result?;
    unload?;
    destroy?;
    Ok(evidence)
}
fn run(
    host: &mut CompatibilityHost,
    binding: &Binding,
    context: &GpuContext,
    control: super::RenderControl,
    world: u64,
) -> Result<Value, String> {
    let table = AssetTable::load(binding.root(), &fixture::graph(binding)?)?;
    let (sprite, script) = fixture::world(host, world)?;
    host.open_phase_session(world).map_err(|e| e.to_string())?;
    host.load_fixture(&json!({"world":world,"requirements":{"execution_profile":"native_phase_context_v1"},"assemblies":[binding.probe()],"handles":{"sprite":sprite,"sprite-script":script},"graph":{"nodes":[{"id":"sprite","active":true,"template":false}],"components":[{"id":"sprite-script","object":"sprite","script":{"type_name":"ProviderSpriteProbe","fields":{}}}],"assets":[],"aliases":{}}})).map_err(|e|e.to_string())?;
    let mut gpu = gpu::Renderer::new(context, &table, control)?;
    let first = capture(host, world, &table, &mut gpu)?;
    require(
        pixel(&first, 34, 24) == [0, 255, 0, 255] && pixel(&first, 34, 40) == [0, 0, 255, 255],
        "actual atlas UV and sprite rect/pivot/PPU pixels",
    )?;
    let ordered = pixel(&first, 18, 24);
    require(
        ordered[1] > 180 && ordered[1] < 195 && ordered[0] == 0,
        "tile extraction and sprite sorting/alpha must affect the same pixels",
    )?;
    let images = mutations(host, world, &table, &mut gpu, first)?;
    gpu.retain(binding, &images)?;
    Ok(
        json!({"adapter":format!("{:?}",context.adapter().get_info()),"captures":images.iter().map(|v|worldrunner::ContentHash::of(v).to_hex()).collect::<Vec<_>>(),"pipeline":"AssetTable.extract/DynamicQuad/SceneGpu/unlit-alpha/CopyOutput/Readback","scope":"offscreen sprite and rectangular tile; material/shader/font resource identities pending"}),
    )
}
fn mutations(
    host: &mut CompatibilityHost,
    world: u64,
    table: &AssetTable,
    gpu: &mut gpu::Renderer<'_>,
    first: Vec<u8>,
) -> Result<Vec<Vec<u8>>, String> {
    host.advance_rational(world, 1, 50)
        .map_err(|e| e.to_string())?;
    let moved = capture(host, world, table, gpu)?;
    require(
        pixel(&moved, 34, 24) == [0, 0, 0, 255] && pixel(&moved, 48, 24) == [0, 255, 0, 255],
        "native TRS mutation must move extracted pixels",
    )?;
    host.advance_rational(world, 1, 50)
        .map_err(|e| e.to_string())?;
    let tinted = capture(host, world, table, gpu)?;
    require(
        pixel(&moved, 8, 24) == [0, 255, 0, 255],
        "actual native tile TRS mutation must move extracted pixels",
    )?;
    let tint = pixel(&tinted, 48, 24);
    require(
        tint[1] > 80 && tint[1] < 110 && tint[0] == 0,
        "native tint and alpha reach unlit-alpha blending",
    )?;
    host.advance_rational(world, 1, 50)
        .map_err(|e| e.to_string())?;
    let hidden = capture(host, world, table, gpu)?;
    require(
        pixel(&hidden, 48, 24) == [0, 0, 0, 255],
        "disabled native renderer must disappear",
    )?;
    host.advance_rational(world, 1, 50)
        .map_err(|e| e.to_string())?;
    let flipped = capture(host, world, table, gpu)?;
    require(
        pixel(&flipped, 48, 24) == [0, 0, 255, 255],
        "native UV flip must alter pixels",
    )?;
    host.advance_rational(world, 1, 50)
        .map_err(|e| e.to_string())?;
    let inactive = capture(host, world, table, gpu)?;
    require(
        pixel(&inactive, 48, 24) == [0, 0, 0, 255],
        "inactive native object must disappear",
    )?;
    Ok(vec![first, moved, tinted, hidden, flipped, inactive])
}

fn capture(
    host: &CompatibilityHost,
    world: u64,
    table: &AssetTable,
    gpu: &mut gpu::Renderer<'_>,
) -> Result<Vec<u8>, String> {
    let snapshot = host.capture_snapshot(world).map_err(|e| e.to_string())?;
    host.validate_snapshot(world, &snapshot, true)
        .map_err(|e| e.to_string())?;
    let contribution = table.extract(
        snapshot.value(),
        Viewport {
            width: 64,
            height: 64,
        },
    )?;
    let result = gpu.capture(&contribution);
    host.validate_snapshot(world, &snapshot, true)
        .map_err(|e| e.to_string())?;
    host.release_snapshot(world, &snapshot)
        .map_err(|e| e.to_string())?;
    result
}
fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    let offset = (y * 64 + x) * 4;
    bytes[offset..offset + 4]
        .try_into()
        .expect("bounded captured pixel")
}
