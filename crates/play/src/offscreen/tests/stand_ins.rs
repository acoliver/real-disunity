//! Draws that read a stand-in texture are counted by property and reason, through the adapter and
//! render-program's offscreen state on a real adapter. The camera shaders read `_Albedo`. A shader
//! file named `default_white` declares it with the default `white`; `camera` does not declare it.

use std::collections::BTreeMap;

use glam::{Quat, Vec3};
use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureDimension, TextureSampler};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue as WorldValue, RenderTextureId, ShaderProperties, TextureRef,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{MeshRenderer, World};

use super::super::adapter::OffscreenAdapter;
use super::super::assets::{AssetError, Assets};
use super::super::frame::{Report, Settings};
use super::gpu::{MESH, Quad, Rig, Synthetic, shader, target, vector};
use super::support::{camera, material, renderer_with, set, texture_slot, world};

/// `GraphicsFormat` 45, one 16-bit float channel.
const HEIGHT: u32 = 45;
/// The stored texture identity of a texture the player's managed engine would store.
const STORED: u64 = 1 << 40;

/// A rig whose cameras draw the `Gamma` pass of the camera shaders, the one that samples `_Albedo`.
pub(super) fn gamma_rig() -> Rig {
    let mut rig = Rig::new();
    let settings = Settings {
        light_modes: vec!["Gamma".to_owned()],
        ..Settings::default()
    };
    rig.adapter = OffscreenAdapter::with_supply(Synthetic, settings);
    rig
}

/// A camera render into a new target of a quad drawn with the material of `shader_file`, whose
/// `_Albedo` slot is `slot` (unlisted for `None`).
pub(super) fn camera_scene(
    world: &mut World,
    shader_file: &str,
    slot: Option<TextureRef>,
) -> RenderTextureId {
    let heights = target(world, HEIGHT);
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let shader = shader(world, shader_file);
    let material = material(world, shader);
    vector(world, material, "_Tint", [0.5, 0.0, 0.0, 1.0]);
    if let Some(texture) = slot {
        set(
            world,
            material,
            WorldValue::Texture("_Albedo".into(), texture_slot(texture)),
        );
    }
    let mut block = ShaderProperties::default();
    block
        .set(WorldValue::Vector("_Place".into(), [0.5, 1.0, -0.5, 0.0]))
        .unwrap();
    let placed = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        property_block: Some(block),
        slot_blocks: std::collections::BTreeMap::new(),
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, placed);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: heights,
        })
        .unwrap();
    heights
}

/// The report of a frame that draws `shader_file` with `_Albedo` set to `slot`.
fn report_of(assets: &dyn Assets, shader_file: &str, slot: Option<TextureRef>) -> Report {
    let mut rig = gamma_rig();
    let mut world = world();
    camera_scene(&mut world, shader_file, slot);
    let (adapted, _operations, _reads) = rig.frame_with(assets, &mut world, &[]);
    assert_eq!(adapted.report.missing_textures, BTreeMap::new());
    assert_eq!(drawn(&adapted.report), 1, "the camera draws the quad");
    adapted.report
}

/// The draws the cameras of the frame recorded.
fn drawn(report: &Report) -> usize {
    report.camera_draws.iter().map(|camera| camera.drawn).sum()
}

fn by_property(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
    entries
        .iter()
        .map(|(name, count)| ((*name).to_owned(), *count))
        .collect()
}

/// Stored textures that hold a texture of an engine format the host does not decode.
struct Undecodable;

impl Assets for Undecodable {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Quad.mesh(identity, known)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok((identity == STORED && known == 0).then(|| PlayerTexture {
            format: 999,
            width: 1,
            height: 1,
            mips: 1,
            linear: true,
            generation: 1,
            distance_spread: 0,
            depth: 1,
            dimension: TextureDimension::D2,
            graphics_format: 0,
            bytes: vec![0; 4],
        }))
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

#[test]
fn a_draw_that_reads_a_declared_default_is_counted_under_its_property_and_reason() {
    let unlisted = report_of(&Quad, "default_white", None);
    assert_eq!(unlisted.stand_ins.draws, 1);
    assert_eq!(
        unlisted.stand_ins.by_property,
        by_property(&[("_Albedo unset", 1)])
    );
    assert_eq!(unlisted.unresolved_textures, BTreeMap::new());
    // A slot the game's own data leaves null reads the shader's declared default, as the original.
    let none = report_of(&Quad, "default_white", Some(TextureRef::None));
    assert_eq!(none.stand_ins.draws, 1);
    assert_eq!(
        none.stand_ins.by_property,
        by_property(&[("_Albedo declared-default", 1)])
    );
    assert_eq!(none.unresolved_textures, BTreeMap::new());
}

#[test]
fn a_slot_that_names_a_texture_the_engine_did_not_send_is_counted_as_not_loaded() {
    let unsent = report_of(&Quad, "default_white", Some(TextureRef::Unsent));
    assert_eq!(unsent.stand_ins.draws, 1);
    assert_eq!(
        unsent.stand_ins.by_property,
        by_property(&[("_Albedo not-loaded", 1)])
    );
    assert_eq!(unsent.unresolved_textures, BTreeMap::new());
}

/// A camera that draws nothing, its culling mask names no layer, and clears a new target, which
/// writes it.
fn written_target(world: &mut World) -> RenderTextureId {
    let written = target(world, 8);
    let clearing = Camera {
        culling_mask: 0,
        ..Camera::default()
    };
    let camera = camera(world, clearing, Vec3::ZERO, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: written,
        })
        .unwrap();
    written
}

#[test]
fn a_draw_that_reads_a_real_texture_is_not_counted() {
    let mut rig = gamma_rig();
    let mut world = world();
    let real = written_target(&mut world);
    camera_scene(&mut world, "default_white", Some(TextureRef::Render(real)));
    let (adapted, _operations, _reads) = rig.frame(&mut world, &[]);
    assert_eq!(adapted.report.missing_textures, BTreeMap::new());
    assert_eq!(drawn(&adapted.report), 1, "the camera draws the quad");
    assert_eq!(adapted.report.stand_ins.draws, 0);
    assert!(adapted.report.stand_ins.by_property.is_empty());
}

#[test]
fn a_render_texture_an_earlier_frame_wrote_is_a_real_texture() {
    let mut rig = gamma_rig();
    let mut world = world();
    let real = written_target(&mut world);
    let (_adapted, operations, _reads) = rig.frame(&mut world, &[]);
    rig.run(operations);
    camera_scene(&mut world, "default_white", Some(TextureRef::Render(real)));
    let (adapted, _operations, _reads) = rig.frame(&mut world, &[]);
    assert_eq!(drawn(&adapted.report), 1, "the camera draws the quad");
    assert_eq!(adapted.report.stand_ins.draws, 0);
}

#[test]
fn a_draw_that_reads_the_grey_for_an_undeclared_slot_is_counted_as_undeclared() {
    let report = report_of(&Quad, "camera", None);
    assert_eq!(report.stand_ins.draws, 1);
    assert_eq!(
        report.stand_ins.by_property,
        by_property(&[("_Albedo undeclared", 1)])
    );
    assert_eq!(report.unresolved_textures, BTreeMap::new());
}

#[test]
fn a_texture_the_engine_names_and_the_host_holds_no_pixels_of_is_not_called_unset() {
    let missing = report_of(&Quad, "camera", Some(TextureRef::Asset(STORED)));
    assert_eq!(
        missing.stand_ins.by_property,
        by_property(&[("_Albedo exists-not-loaded", 1)]),
        "the engine stored no pixels for it"
    );
    let undecodable = report_of(&Undecodable, "camera", Some(TextureRef::Asset(STORED)));
    assert_eq!(
        undecodable.stand_ins.by_property,
        by_property(&[("_Albedo unsupported-format", 1)]),
        "the engine stored a format the host does not decode"
    );
    assert_eq!(undecodable.stand_ins.draws, 1);
}

/// A camera render as `camera_scene`, with a property block of the renderer that also sets
/// `_Albedo` to `block_albedo`.
fn block_scene(world: &mut World, shader_file: &str, block_albedo: TextureRef) {
    let heights = target(world, HEIGHT);
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let shader = shader(world, shader_file);
    let material = material(world, shader);
    vector(world, material, "_Tint", [0.5, 0.0, 0.0, 1.0]);
    let mut block = ShaderProperties::default();
    block
        .set(WorldValue::Vector("_Place".into(), [0.5, 1.0, -0.5, 0.0]))
        .unwrap();
    block
        .set(WorldValue::Texture(
            "_Albedo".into(),
            texture_slot(block_albedo),
        ))
        .unwrap();
    let placed = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        property_block: Some(block),
        slot_blocks: std::collections::BTreeMap::new(),
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, placed);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: heights,
        })
        .unwrap();
}

#[test]
fn a_property_block_that_names_a_texture_the_host_does_not_hold_is_exists_not_loaded() {
    let mut rig = gamma_rig();
    let mut world = world();
    // The material lists no `_Albedo`, so the draw reads the shader's declared white while the
    // renderer's block names a stored texture the engine holds no pixels of.
    block_scene(&mut world, "default_white", TextureRef::Asset(STORED));
    let (adapted, _operations, _reads) = rig.frame_with(&Quad, &mut world, &[]);
    assert_eq!(adapted.report.missing_textures, BTreeMap::new());
    assert_eq!(drawn(&adapted.report), 1, "the camera draws the quad");
    assert_eq!(adapted.report.stand_ins.draws, 1);
    assert_eq!(
        adapted.report.stand_ins.by_property,
        by_property(&[("_Albedo exists-not-loaded", 1)]),
        "the block names a texture the host does not hold, so it is not 'unset'"
    );
}

#[test]
fn a_draw_that_reads_a_render_texture_nothing_has_written_is_counted_as_a_stand_in() {
    let mut rig = gamma_rig();
    let mut world = world();
    let unwritten = target(&mut world, 8);
    camera_scene(
        &mut world,
        "default_white",
        Some(TextureRef::Render(unwritten)),
    );
    let (adapted, _operations, _reads) = rig.frame(&mut world, &[]);
    assert_eq!(drawn(&adapted.report), 1, "the camera draws the quad");
    assert_eq!(
        adapted.report.stand_ins.draws, 1,
        "the draw samples texels no draw wrote"
    );
}
