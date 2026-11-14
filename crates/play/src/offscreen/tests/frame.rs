//! The mapping of a snapshot onto an offscreen frame.

use glam::{Quat, Vec3};
use std::collections::BTreeMap;
use worldrunner_render_program::ids::{CameraId, MeshId, RendererId, ShaderId, TextureId};
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::sampler::{Filter, SamplerState, Wrap};
use worldrunner_render_program::unity::offscreen::frame::Record;
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::graphics::{
    MaterialEdit, OffscreenWork, PropertyValue as WorldValue, RenderTextureId, ShaderProperties,
    TextureRef, WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::presentation::{Camera, CameraClear};
use worldrunner_world_simulation::{ComponentHandle, MeshRenderer, World};

use super::super::frame::{BLIT_MESH, Placed, RendererSkip, Settings, Skip, adapt};
use super::super::ids;
use super::support::{
    available, camera, compute_buffer, compute_shader, dispatch, material, render_texture,
    renderer, renderer_with, set, shader, snapshot, texture_slot, world,
};

const MESH: u64 = (1 << 41) + 3;

fn settings() -> Settings {
    Settings {
        light_modes: vec!["Alpha".to_owned(), "Fallback".to_owned()],
        untagged_light_mode: None,
    }
}

fn draw(world: &mut World, camera: ComponentHandle, target: RenderTextureId) {
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
}

fn blit(
    world: &mut World,
    source: TextureRef,
    target: RenderTextureId,
    material: Option<worldrunner_world_simulation::graphics::MaterialId>,
    pass: i32,
) {
    world
        .record_offscreen(OffscreenWork::Blit {
            source,
            target,
            material,
            pass,
        })
        .unwrap();
}

fn passes(records: &[Record]) -> Vec<usize> {
    records
        .iter()
        .map(|record| match record {
            Record::Blit { pass, .. } => *pass,
            other => panic!("{other:?} is not a blit"),
        })
        .collect()
}

#[test]
fn a_blit_of_every_pass_is_a_record_for_each_pass_in_order_and_others_stay_single() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    let shader = shader(&mut world, "Blit");
    let material = material(&mut world, shader);
    blit(&mut world, TextureRef::Render(a), b, Some(material), -1);
    blit(&mut world, TextureRef::Render(b), a, Some(material), 1);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 3, &[]), &settings());
    assert_eq!(passes(&adapted.frame.records), [0, 1, 2, 1]);
    let Record::Blit {
        source,
        target,
        material: used,
        ..
    } = &adapted.frame.records[0]
    else {
        panic!("a blit");
    };
    assert_eq!(*source, ids::render_texture(a));
    assert_eq!(*target, ids::render_texture(b));
    assert_eq!(*used, Some(ids::material(material)));
    assert_eq!((adapted.report.records, adapted.report.built), (2, 4));
    assert_eq!(adapted.frame.blit_mesh, Some(BLIT_MESH));
}

#[test]
fn a_blit_without_a_material_is_one_record_that_copies() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    blit(&mut world, TextureRef::Render(a), b, None, -1);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 3, &[]), &settings());
    assert!(matches!(
        adapted.frame.records.as_slice(),
        [Record::Blit {
            material: None,
            pass: 0,
            ..
        }]
    ));
}

#[test]
fn records_keep_the_order_the_script_called_them_in() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    draw(&mut world, camera, a);
    blit(&mut world, TextureRef::Render(a), b, None, 0);
    world
        .record_offscreen(OffscreenWork::CopyTexture {
            source: TextureRef::Render(b),
            source_element: 0,
            source_mip: 0,
            target: a,
            target_element: 0,
            target_mip: 0,
        })
        .unwrap();
    draw(&mut world, camera, b);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    let kinds: Vec<_> = adapted
        .frame
        .records
        .iter()
        .map(|record| match record {
            Record::CameraRender { .. } => "camera",
            Record::Blit { .. } => "blit",
            Record::CopyTexture { .. } => "copy",
            Record::Clear { .. } => "clear",
            Record::Draws(_) => "draws",
            Record::Globals(_) => "globals",
            Record::Procedural(_) => "procedural",
        })
        .collect();
    assert_eq!(kinds, ["camera", "blit", "copy", "camera"]);
    // The camera draws into targets of one size, so it is one camera of the frame.
    assert_eq!(adapted.frame.cameras.len(), 1);
}

#[test]
fn records_that_name_what_no_longer_exists_are_skipped_by_reason_and_the_rest_stay() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    let gone_target = render_texture(&mut world, [8, 8]);
    let gone_source = render_texture(&mut world, [8, 8]);
    let live_camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let shader = shader(&mut world, "Blit");
    let live = material(&mut world, shader);
    let gone_material = material(&mut world, shader);
    draw(&mut world, live_camera, gone_target);
    blit(&mut world, TextureRef::Render(gone_source), a, None, 0);
    blit(&mut world, TextureRef::Render(a), gone_target, None, 0);
    blit(&mut world, TextureRef::Render(a), b, Some(gone_material), 0);
    blit(&mut world, TextureRef::None, b, None, 0);
    blit(&mut world, TextureRef::Asset(77), b, None, 0);
    blit(&mut world, TextureRef::Render(a), b, Some(live), 0);
    world.destroy_material(gone_material).unwrap();
    let mut snapshot = snapshot(&mut world);
    // The world keeps a texture its frame's records name until the snapshot, so a snapshot that
    // lacks one comes from elsewhere (the wire); the adapter skips the records that name it.
    snapshot.render_textures.remove(&gone_target);
    snapshot.render_textures.remove(&gone_source);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    let skipped: Vec<_> = adapted
        .report
        .skipped
        .iter()
        .map(|(k, n)| (*k, *n))
        .collect();
    assert_eq!(
        skipped,
        [
            (Skip::StaleTarget, 2),
            (Skip::StaleSource, 1),
            (Skip::StaleMaterial, 1),
            (Skip::SourceUnavailable, 1),
            (Skip::NoBlitSource, 1),
        ]
    );
    assert_eq!(adapted.frame.records.len(), 1, "only the live blit is left");
    assert_eq!(adapted.report.records, 7);
}

#[test]
fn a_release_of_a_temporary_nobody_allocated_is_skipped_by_reason_and_the_draws_stay() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let object = super::support::object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let handle = world
        .add_component(
            object,
            worldrunner_world_simulation::ComponentDesc::Camera(Camera::default()),
        )
        .unwrap();
    world
        .record_offscreen(OffscreenWork::SetGlobalKeyword {
            name: "FOG".to_owned(),
            enabled: true,
        })
        .unwrap();
    draw(&mut world, handle, target);
    world
        .record_offscreen(OffscreenWork::ReleaseTemporary { name_id: 7 })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert_eq!(adapted.report.records, 3);
    assert_eq!(
        adapted.report.skipped.get(&Skip::UnknownTemporary),
        Some(&1)
    );
    assert_eq!(adapted.report.skipped.len(), 1);
    // The keyword reaches the camera render through a globals record in front of it.
    assert_eq!(adapted.frame.records.len(), 2);
    assert!(matches!(adapted.frame.records[0], Record::Globals(_)));
    assert!(matches!(
        adapted.frame.records[1],
        Record::CameraRender { .. }
    ));
    assert_eq!(adapted.report.built, 1);
}

/// One of every script command, recorded in a world that has a camera and two render textures.
fn every_script_command(world: &mut World, camera: ComponentHandle, target: RenderTextureId) {
    let other = render_texture(world, [4, 4]);
    let desc = WorldRenderTexture {
        width: 4,
        height: 4,
        format: 8,
        depth_bits: 0,
        mip_count: 1,
        srgb: false,
        dimension: 2,
        volume_depth: 1,
        sampler: WorldSampler::default(),
        generation: 0,
    };
    let commands = [
        OffscreenWork::SetRenderTarget {
            color: vec![worldrunner_world_simulation::graphics::TargetRef::Render(
                target,
            )],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
        OffscreenWork::SetRenderTarget {
            color: vec![worldrunner_world_simulation::graphics::TargetRef::Render(
                other,
            )],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value: [0.0; 4],
            depth_value: 1.0,
            stencil_value: 0,
        },
        OffscreenWork::SetGlobal(WorldValue::Float("_A".to_owned(), 1.0)),
        OffscreenWork::SetGlobal(WorldValue::Int("_B".to_owned(), 2)),
        OffscreenWork::SetGlobal(WorldValue::Vector("_C".to_owned(), [1.0; 4])),
        OffscreenWork::SetGlobal(WorldValue::Texture(
            "_Map".to_owned(),
            texture_slot(TextureRef::Render(other)),
        )),
        OffscreenWork::SetGlobalKeyword {
            name: "FOG".to_owned(),
            enabled: true,
        },
        OffscreenWork::SetGlobalKeyword {
            name: "FOG".to_owned(),
            enabled: false,
        },
        OffscreenWork::AllocateTemporary { name_id: 7, desc },
        OffscreenWork::ReleaseTemporary { name_id: 7 },
        OffscreenWork::SetupCamera {
            camera,
            view: glam::Mat4::IDENTITY.to_cols_array(),
            projection: glam::Mat4::IDENTITY.to_cols_array(),
            target: worldrunner_world_simulation::graphics::TargetRef::Render(target),
        },
        OffscreenWork::Cull {
            id: 5,
            camera,
            culling_mask: u32::MAX,
            planes: Vec::new(),
            options: 0,
        },
        OffscreenWork::Cull {
            id: 6,
            camera,
            culling_mask: 1,
            planes: Vec::new(),
            options: 0,
        },
    ];
    for command in commands {
        world.record_offscreen(command).unwrap();
    }
    let made = world
        .create_renderer_list(
            worldrunner_world_simulation::graphics::RendererListDesc::Draw {
                cull: 5,
                pass_tags: vec!["Alpha".to_owned()],
                queue_min: 0,
                queue_max: 5000,
                layer_mask: u32::MAX,
                rendering_layer_mask: u32::MAX,
                sorting: 0,
                per_object: 0,
                override_material: None,
                override_pass: -1,
            },
        )
        .unwrap();
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list: made })
        .unwrap();
}

#[test]
fn every_script_command_is_run_and_counted_by_kind() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let object = super::support::object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let handle = world
        .add_component(
            object,
            worldrunner_world_simulation::ComponentDesc::Camera(Camera::default()),
        )
        .unwrap();
    every_script_command(&mut world, handle, target);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    let counts = |pairs: &[(&str, usize)]| -> BTreeMap<String, usize> {
        pairs
            .iter()
            .map(|(kind, count)| ((*kind).to_owned(), *count))
            .collect()
    };
    assert_eq!(
        adapted.report.built_by_kind,
        counts(&[
            ("AllocateTemporary", 1),
            ("ClearRenderTarget", 1),
            ("CreateRendererList", 1),
            ("Cull", 2),
            ("DrawRendererList", 1),
            ("ReleaseTemporary", 1),
            ("SetGlobal", 4),
            ("SetGlobalKeyword", 2),
            ("SetRenderTarget", 2),
            ("SetupCamera", 1),
        ])
    );
    assert_eq!(adapted.report.records, 16);
    assert!(adapted.report.skipped.is_empty());
    assert_eq!(adapted.frame.records.len(), 1, "the one clear");
}

#[test]
fn a_camera_destroyed_after_it_rendered_is_skipped() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let object = super::support::object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let handle = world
        .add_component(
            object,
            worldrunner_world_simulation::ComponentDesc::Camera(Camera::default()),
        )
        .unwrap();
    draw(&mut world, handle, target);
    world.destroy_object(object).unwrap();
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert_eq!(adapted.report.skipped.get(&Skip::StaleCamera), Some(&1));
    assert!(adapted.frame.records.is_empty());
}

#[test]
fn a_blit_whose_shader_could_not_be_loaded_is_skipped() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    let shader = shader(&mut world, "Blit");
    let material = material(&mut world, shader);
    blit(&mut world, TextureRef::Render(a), b, Some(material), -1);
    let snapshot = snapshot(&mut world);
    let mut unavailable = available(&snapshot, 1, &[]);
    unavailable.shaders.clear();
    let adapted = adapt(&snapshot, &unavailable, &settings());
    assert_eq!(
        adapted.report.skipped.get(&Skip::ShaderUnavailable),
        Some(&1)
    );
    assert!(adapted.frame.records.is_empty());
}

#[test]
fn a_render_texture_keeps_its_format_depth_mips_srgb_and_generation() {
    let mut world = world();
    let description = WorldRenderTexture {
        width: 32,
        height: 16,
        format: 4,
        depth_bits: 24,
        mip_count: 4,
        srgb: true,
        dimension: 2,
        volume_depth: 1,
        sampler: WorldSampler::default(),
        generation: 0,
    };
    let texture = world.create_render_texture(description).unwrap();
    world.update_render_texture(texture, description).unwrap();
    let unused = render_texture(&mut world, [4, 4]);
    let (b, _) = (render_texture(&mut world, [8, 8]), unused);
    blit(&mut world, TextureRef::Render(texture), b, None, 0);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    let specs = &adapted.frame.render_textures;
    assert_eq!(
        specs.len(),
        2,
        "the texture nothing named is not in the frame"
    );
    let spec = specs
        .iter()
        .find(|spec| spec.id == ids::render_texture(texture))
        .unwrap();
    assert_eq!(spec.size, [32, 16]);
    assert_eq!(
        (spec.layers(), spec.graphics_format, spec.depth_bits),
        (1, 4, 24)
    );
    assert_eq!((spec.mips, spec.srgb, spec.generation), (4, true, 2));
    assert_eq!(
        adapted.report.formats.iter().copied().collect::<Vec<_>>(),
        [4, 8]
    );
    let census: Vec<_> = adapted
        .report
        .census
        .iter()
        .filter(|(name, _)| name.starts_with("frame render texture of format"))
        .map(|(name, count)| (name.as_str(), *count))
        .collect();
    assert_eq!(
        census,
        [
            ("frame render texture of format 4", 1),
            ("frame render texture of format 8", 1)
        ]
    );
}

#[test]
fn a_material_carries_its_shader_keywords_properties_and_queue() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    let shader = shader(&mut world, "Blit");
    let named = material(&mut world, shader);
    let default = material(&mut world, shader);
    for name in ["_MULTIPLY", "_ADD"] {
        world
            .edit_material(
                named,
                MaterialEdit::Keyword {
                    name: name.into(),
                    enabled: true,
                },
            )
            .unwrap();
    }
    world
        .edit_material(named, MaterialEdit::RenderQueue(2450))
        .unwrap();
    set(
        &mut world,
        named,
        WorldValue::Vector("_Tint".into(), [1.0, 0.0, 0.5, 1.0]),
    );
    blit(&mut world, TextureRef::Render(a), b, Some(named), 0);
    blit(&mut world, TextureRef::Render(a), b, Some(default), 0);
    let snapshot = snapshot(&mut world);
    let mut offered = available(&snapshot, 1, &[]);
    for info in offered.shaders.values_mut() {
        info.queue = 3000;
    }
    let adapted = adapt(&snapshot, &offered, &settings());
    let materials = &adapted.frame.materials;
    let used = &materials[&ids::material(named)];
    assert_eq!(used.shader, Some(ShaderId(shader.to_bits())));
    assert_eq!(
        used.keywords,
        ["_ADD", "_MULTIPLY"],
        "sorted, as the world keeps them"
    );
    assert_eq!(
        used.properties.values.get("_Tint"),
        Some(&PropertyValue::Vector([1.0, 0.0, 0.5, 1.0]))
    );
    assert_eq!(used.render_queue, 2450);
    assert_eq!(
        materials[&ids::material(default)].render_queue,
        3000,
        "a material that names no queue takes its shader's"
    );
}

#[test]
fn globals_and_global_keywords_are_in_the_frame() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    world
        .set_global_property(WorldValue::Float("_Fog".into(), 0.25))
        .unwrap();
    world
        .set_global_property(WorldValue::Texture(
            "_Noise".into(),
            texture_slot(TextureRef::Render(a)),
        ))
        .unwrap();
    world.set_global_keyword("FOG_ON", true).unwrap();
    blit(&mut world, TextureRef::Render(a), b, None, 0);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert_eq!(
        adapted.frame.globals.values.get("_Fog"),
        Some(&PropertyValue::Float(0.25))
    );
    assert_eq!(
        adapted.frame.globals.textures["_Noise"].texture,
        ids::render_texture(a)
    );
    assert_eq!(adapted.frame.global_keywords, ["FOG_ON"]);
}

#[test]
fn a_camera_maps_its_clear_flags_mask_pass_list_and_front_face() {
    let cases = [
        (CameraClear::Skybox, Some([0.1, 0.2, 0.3, 1.0]), true),
        (CameraClear::SolidColor, Some([0.1, 0.2, 0.3, 1.0]), true),
        (CameraClear::DepthOnly, None, true),
        (CameraClear::Nothing, None, false),
    ];
    for (clear, color, depth) in cases {
        let mut world = world();
        let target = render_texture(&mut world, [8, 8]);
        let settings_of = Camera {
            clear,
            background: [0.1, 0.2, 0.3, 1.0],
            culling_mask: 0b1010,
            ..Camera::default()
        };
        let camera = camera(&mut world, settings_of, Vec3::ZERO, Quat::IDENTITY);
        draw(&mut world, camera, target);
        let snapshot = snapshot(&mut world);
        let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
        let made = &adapted.frame.cameras[&CameraId(0)];
        assert_eq!(
            (made.clear_color, made.clear_depth),
            (color, depth),
            "{clear:?}"
        );
        assert_eq!(made.culling_mask, 0b1010);
        assert_eq!(
            made.pass,
            PassSelector::Tag {
                key: "LightMode".to_owned(),
                values: vec!["Alpha".to_owned(), "Fallback".to_owned()],
            }
        );
        // Unity's clockwise front faces are counter-clockwise in the image, which is turned over.
        assert_eq!(made.front_face, wgpu::FrontFace::Ccw);
    }
}

#[test]
fn a_camera_counts_passes_without_a_light_mode_as_the_setting_says() {
    let mut world = world();
    let target = render_texture(&mut world, [4, 4]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    draw(&mut world, camera, target);
    let snapshot = snapshot(&mut world);
    let settings = Settings {
        untagged_light_mode: Some("Fallback".to_owned()),
        ..settings()
    };
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings);
    assert_eq!(
        adapted.frame.cameras[&CameraId(0)].pass,
        PassSelector::TagWithUntagged {
            key: "LightMode".to_owned(),
            values: vec!["Alpha".to_owned(), "Fallback".to_owned()],
            untagged: "Fallback".to_owned(),
        }
    );
}

#[test]
fn one_camera_drawing_into_targets_of_two_shapes_is_two_cameras_of_the_frame() {
    let mut world = world();
    let square = render_texture(&mut world, [8, 8]);
    let wide = render_texture(&mut world, [16, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    draw(&mut world, camera, square);
    draw(&mut world, camera, wide);
    draw(&mut world, camera, square);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    let used: Vec<_> = adapted
        .frame
        .records
        .iter()
        .map(|record| match record {
            Record::CameraRender { camera, .. } => *camera,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(used, [CameraId(0), CameraId(1), CameraId(0)]);
    assert_eq!(adapted.frame.cameras.len(), 2);
}

#[test]
fn a_camera_with_a_partial_viewport_is_skipped() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let half = Camera {
        viewport: [0.0, 0.0, 0.5, 1.0],
        ..Camera::default()
    };
    let camera = camera(&mut world, half, Vec3::ZERO, Quat::IDENTITY);
    draw(&mut world, camera, target);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert_eq!(adapted.report.skipped.get(&Skip::PartialViewport), Some(&1));
}

struct Drawn {
    snapshot: worldrunner_world_simulation::graphics::GraphicsSnapshot,
}

/// A camera that sees layers 1 and 2 and three renderers: one on layer 1 with a property block
/// and a render queue of its own, one on layer 3, which the camera does not see, and one on layer
/// 2 whose first slot has no material.
fn scene() -> (
    Drawn,
    Vec<worldrunner_world_simulation::graphics::MaterialId>,
) {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(
        &mut world,
        Camera {
            culling_mask: 0b110,
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let shader = shader(&mut world, "Draw");
    let (first, second) = (material(&mut world, shader), material(&mut world, shader));
    let mut block = ShaderProperties::default();
    block
        .set(WorldValue::Vector("_Tint".into(), [0.0, 1.0, 0.0, 1.0]))
        .unwrap();
    let with_block = MeshRenderer {
        material_count: 3,
        slot_materials: vec![Some(first), Some(second), None],
        property_block: Some(block),
        render_queue: Some(2600),
        ..MeshRenderer::default()
    };
    renderer_with(&mut world, (1, true), MESH, with_block);
    renderer(&mut world, 3, MESH + 1, vec![Some(first)]);
    renderer(&mut world, 2, MESH + 2, vec![None, Some(first)]);
    draw(&mut world, camera, target);
    (
        Drawn {
            snapshot: snapshot(&mut world),
        },
        vec![first, second],
    )
}

#[test]
fn a_renderer_carries_its_mesh_slots_layer_block_and_per_draw_values() {
    let (scene, materials) = scene();
    let adapted = adapt(
        &scene.snapshot,
        &available(&scene.snapshot, 1, &[]),
        &settings(),
    );
    let renderer = &adapted.frame.renderers[&RendererId(0)];
    assert_eq!(renderer.mesh, MeshId(MESH));
    assert_eq!(renderer.layer, 1);
    assert_eq!(
        renderer.materials.len(),
        2,
        "the slot after the last drawn one is not listed"
    );
    assert_eq!(
        renderer.block.values.get("_Tint"),
        Some(&PropertyValue::Vector([0.0, 1.0, 0.0, 1.0]))
    );
    assert!(
        renderer
            .per_draw
            .values
            .get("unity_ObjectToWorld")
            .is_some()
    );
    // The renderer's own queue is on its materials, not on the plain ones.
    let queues: Vec<_> = renderer
        .materials
        .iter()
        .map(|id| adapted.frame.materials[id].render_queue)
        .collect();
    assert_eq!(queues, [2600, 2600]);
    assert!(
        renderer
            .materials
            .iter()
            .all(|id| !materials.iter().any(|plain| *id == ids::material(*plain)))
    );
}

#[test]
fn renderers_the_cameras_do_not_see_are_not_in_the_frame_and_a_gap_before_a_slot_is_named() {
    let (scene, _) = scene();
    let adapted = adapt(
        &scene.snapshot,
        &available(&scene.snapshot, 1, &[]),
        &settings(),
    );
    let listed: Vec<_> = adapted.frame.renderers.keys().copied().collect();
    assert_eq!(
        listed,
        [RendererId(0)],
        "layer 3 is outside the mask and layer 2 has a gap"
    );
    assert_eq!(
        adapted
            .report
            .renderers_skipped
            .iter()
            .map(|(k, n)| (*k, *n))
            .collect::<Vec<_>>(),
        [(RendererSkip::EmptySlot, 1)]
    );
}

#[test]
fn a_renderer_whose_mesh_the_host_does_not_have_is_skipped() {
    let (scene, _) = scene();
    let mut offered = available(&scene.snapshot, 1, &[]);
    offered.meshes.clear();
    let adapted = adapt(&scene.snapshot, &offered, &settings());
    assert!(adapted.frame.renderers.is_empty());
    assert_eq!(
        adapted
            .report
            .renderers_skipped
            .get(&RendererSkip::MeshUnavailable),
        Some(&2)
    );
}

#[test]
fn a_renderer_with_no_material_at_all_is_skipped() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    renderer(&mut world, 0, MESH, vec![None, None]);
    draw(&mut world, camera, target);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert_eq!(
        adapted
            .report
            .renderers_skipped
            .get(&RendererSkip::NoMaterial),
        Some(&1)
    );
}

#[test]
fn a_stored_texture_the_host_can_read_is_named_by_its_identity() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    let shader = shader(&mut world, "Blit");
    let material = material(&mut world, shader);
    set(
        &mut world,
        material,
        WorldValue::Texture("_Lookup".into(), texture_slot(TextureRef::Asset(1 << 40))),
    );
    blit(&mut world, TextureRef::Asset(1 << 40), b, Some(material), 0);
    blit(&mut world, TextureRef::Render(a), b, Some(material), 0);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[1 << 40]), &settings());
    let input = adapted.frame.materials[&ids::material(material)]
        .properties
        .textures["_Lookup"];
    assert_eq!(input.texture, TextureId(1 << 40));
    let Record::Blit { source, .. } = &adapted.frame.records[0] else {
        panic!("a blit");
    };
    assert_eq!(*source, TextureId(1 << 40));
}

#[test]
fn a_texture_property_that_names_no_texture_the_frame_can_read_is_counted_by_name_and_reason() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let shader = shader(&mut world, "Blit");
    let material = material(&mut world, shader);
    for (name, texture) in [
        ("_NotUploaded", TextureRef::Asset(1 << 40)),
        ("_Uploaded", TextureRef::Asset(1 << 41)),
        ("_Nothing", TextureRef::None),
    ] {
        set(
            &mut world,
            material,
            WorldValue::Texture(name.into(), texture_slot(texture)),
        );
    }
    blit(
        &mut world,
        TextureRef::Render(target),
        target,
        Some(material),
        0,
    );
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[1 << 41]), &settings());
    assert_eq!(
        adapted.report.unresolved_textures,
        BTreeMap::from([
            ("_NotUploaded: asset not readable by the host".to_owned(), 1),
            ("_Nothing: no texture set".to_owned(), 1),
        ])
    );
}

#[test]
fn a_render_texture_named_by_a_property_is_read_with_its_own_sampler() {
    let mut world = world();
    let description = WorldRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        depth_bits: 0,
        mip_count: 1,
        srgb: false,
        dimension: 2,
        volume_depth: 1,
        sampler: WorldSampler {
            wrap_u: 1,
            wrap_v: 2,
            wrap_w: 0,
            filter: 0,
            aniso_level: 1,
            mip_bias: 0.0,
        },
        generation: 0,
    };
    let (a, b) = (
        world.create_render_texture(description).unwrap(),
        render_texture(&mut world, [8, 8]),
    );
    world
        .set_global_property(WorldValue::Texture(
            "_Lookup".into(),
            texture_slot(TextureRef::Render(a)),
        ))
        .unwrap();
    blit(&mut world, TextureRef::Render(a), b, None, 0);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert_eq!(
        adapted.frame.globals.textures["_Lookup"].sampler,
        Some(SamplerState {
            wrap_u: Wrap::Clamp,
            wrap_v: Wrap::Mirror,
            filter: Filter::Point,
        })
    );
}

#[test]
fn a_dispatch_is_not_a_record_but_keeps_its_place_among_them_and_brings_its_textures() {
    let mut world = world();
    let (target, read, other) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    let view = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let shader = compute_shader(&mut world, "kernels");
    let buffer = compute_buffer(&mut world, 4);
    draw(&mut world, view, target);
    dispatch(&mut world, shader, (read, buffer), [1, 1, 1]);
    draw(&mut world, view, other);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert_eq!(adapted.frame.records.len(), 2, "the two camera renders");
    assert_eq!(adapted.placements, [Placed { after: 1, work: 1 }]);
    assert_eq!(
        (
            adapted.report.records,
            adapted.report.built,
            adapted.report.dispatches
        ),
        (3, 2, 1)
    );
    assert!(adapted.report.skipped.is_empty());
    let ids: Vec<_> = adapted
        .frame
        .render_textures
        .iter()
        .map(|spec| spec.id)
        .collect();
    assert!(
        ids.contains(&ids::render_texture(read)),
        "a texture only the dispatch reads is in the frame: {ids:?}"
    );
}
