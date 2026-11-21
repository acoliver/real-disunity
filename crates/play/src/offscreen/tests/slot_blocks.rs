//! A property block set for one material slot reaches the draws of that slot only, above the
//! block set for every slot and the material.

use glam::{Quat, Vec3};
use std::collections::BTreeMap;
use worldrunner_disunity_runtime::{
    MeshChannel, MeshChannels, MeshRange, PlayerTexture, TextureSampler,
};
use worldrunner_render_program::ids::RendererId;
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, PropertyValue, ShaderProperties, TextureRef,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{MeshRenderer, World};

use super::super::assets::{AssetError, Assets};
use super::super::frame::adapt;
use super::super::ids;
use super::frame::settings;
use super::gpu::{MESH, Rig, SIDE, shader, vector};
use super::support::{
    available, camera, material, render_texture, renderer_with, snapshot, texture_slot, world,
};

const STORED: u64 = (1 << 40) + 5;
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn block(values: &[(&str, [f32; 4])]) -> ShaderProperties {
    let mut block = ShaderProperties::default();
    for (name, value) in values {
        block
            .set(PropertyValue::Vector((*name).into(), *value))
            .unwrap();
    }
    block
}

fn textured(name: &str, identity: u64) -> ShaderProperties {
    let mut block = ShaderProperties::default();
    let slot = texture_slot(TextureRef::Asset(identity));
    block
        .set(PropertyValue::Texture(name.into(), slot))
        .unwrap();
    block
}

/// Two quads side by side, the left one in the first sub-mesh and the right one in the second.
struct TwoHalves;

impl Assets for TwoHalves {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        if identity != MESH || known != 0 {
            return Ok(None);
        }
        let positions = [
            [-1.0, -1.0],
            [0.0, -1.0],
            [-1.0, 1.0],
            [0.0, 1.0],
            [0.0, -1.0],
            [1.0, -1.0],
            [0.0, 1.0],
            [1.0, 1.0],
        ];
        Ok(Some(MeshChannels {
            generation: 1,
            vertex_count: 8,
            channels: vec![
                MeshChannel {
                    number: 0,
                    dimension: 3,
                    data: positions.iter().flat_map(|[x, y]| [*x, *y, 0.5]).collect(),
                },
                MeshChannel {
                    number: 4,
                    dimension: 2,
                    data: positions.iter().flat_map(|[x, y]| [*x, *y]).collect(),
                },
            ],
            indices: vec![0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7],
            ranges: [0, 6]
                .map(|start| MeshRange {
                    start,
                    count: 6,
                    base_vertex: 0,
                    topology: 0,
                })
                .to_vec(),
        }))
    }

    fn texture(&self, _: u64, _: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok(None)
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// A two slot renderer of the two quads whose material draws red over the whole target, with the
/// blocks given.
fn scene(
    world: &mut World,
    all: Option<ShaderProperties>,
    slots: BTreeMap<u32, ShaderProperties>,
) -> worldrunner_world_simulation::graphics::RenderTextureId {
    let target = render_texture(world, [SIDE, SIDE]);
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let shader = shader(world, "camera");
    let material: MaterialId = material(world, shader);
    vector(world, material, "_Tint", [1.0, 0.0, 0.0, 1.0]);
    vector(world, material, "_Place", [1.0, 1.0, 0.0, 0.0]);
    let renderer = MeshRenderer {
        material_count: 2,
        slot_materials: vec![Some(material), Some(material)],
        property_block: all,
        slot_blocks: slots,
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, renderer);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    target
}

/// The color of the first and the last column of the drawn target, each the same in every row.
fn halves(
    rig: &mut Rig,
    world: &mut World,
    target: worldrunner_world_simulation::graphics::RenderTextureId,
) -> [[u8; 4]; 2] {
    let (adapted, operations, reads) = rig.frame_with(&TwoHalves, world, &[target]);
    assert!(
        adapted.report.failures.is_empty(),
        "{:?}",
        adapted.report.failures
    );
    rig.run(operations);
    let bytes = reads.into_iter().next().unwrap().bytes(&rig.device);
    let side = SIDE as usize;
    let at =
        |x: usize, y: usize| -> [u8; 4] { bytes[(y * side + x) * 4..][..4].try_into().unwrap() };
    for y in 0..side {
        assert_eq!(at(0, y), at(1, y), "row {y}: the left quad is one color");
        assert_eq!(at(2, y), at(3, y), "row {y}: the right quad is one color");
        assert_eq!(at(0, y), at(0, 0));
        assert_eq!(at(3, y), at(3, 0));
    }
    [at(0, 0), at(3, 0)]
}

#[test]
fn a_block_of_one_slot_colors_that_slots_draw_and_the_other_keeps_the_material() {
    let mut rig = Rig::new();
    let mut world = world();
    let slots = BTreeMap::from([(1, block(&[("_Tint", [0.0, 1.0, 0.0, 1.0])]))]);
    let target = scene(&mut world, None, slots);
    assert_eq!(halves(&mut rig, &mut world, target), [RED, GREEN]);
}

#[test]
fn the_block_of_a_slot_wins_over_the_block_of_every_slot_for_the_same_property() {
    let mut rig = Rig::new();
    let mut world = world();
    let all = block(&[("_Tint", [0.0, 0.0, 1.0, 1.0])]);
    let slots = BTreeMap::from([(1, block(&[("_Tint", [0.0, 1.0, 0.0, 1.0])]))]);
    let target = scene(&mut world, Some(all), slots);
    assert_eq!(halves(&mut rig, &mut world, target), [BLUE, GREEN]);
}

#[test]
fn a_slot_block_with_a_texture_gives_that_texture_to_its_slot_only() {
    let mut world = world();
    let slots = BTreeMap::from([
        (1, textured("_MainTex", STORED)),
        (5, textured("_MainTex", STORED)),
    ]);
    scene(&mut world, None, slots);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[STORED]), &settings());
    let renderer = &adapted.frame.renderers[&RendererId(0)];
    let stored = ids::stored_texture(STORED).unwrap();
    assert_eq!(
        renderer.slot_blocks.keys().copied().collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        renderer.slot_blocks[&1].textures["_MainTex"].texture,
        stored
    );
    assert!(renderer.block.textures.is_empty());
    assert!(adapted.report.missing_textures.is_empty());
}
