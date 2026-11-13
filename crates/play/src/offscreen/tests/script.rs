//! The running state of a frame's command lists: render targets, clears, globals, temporary render
//! textures and the camera setup. The worlds are synthetic; each test predicts the records.

use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, Record, RenderDimension, RenderTextureSpec,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue as WorldValue, RenderTextureId, TargetRef, TextureRef,
    WorldRenderTexture, WorldSampler, WorldShader,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{ComponentDesc, ComponentHandle, World};

use super::super::frame::{Adapted, Settings, Skip, adapt};
use super::super::ids;
use super::support::{available, object, render_texture, snapshot, texture_slot, world};

fn run(world: &mut World) -> Adapted {
    let snapshot = snapshot(world);
    adapt(
        &snapshot,
        &available(&snapshot, 1, &[]),
        &Settings::default(),
    )
}

fn record(world: &mut World, work: OffscreenWork) {
    world.record_offscreen(work).unwrap();
}

fn target(color: &[TargetRef], depth: Option<TargetRef>) -> OffscreenWork {
    OffscreenWork::SetRenderTarget {
        color: color.to_vec(),
        depth,
        mip: 0,
        face: -1,
        slice: -1,
    }
}

fn clear(color: bool, depth: bool, stencil: bool) -> OffscreenWork {
    OffscreenWork::ClearRenderTarget {
        color,
        depth,
        stencil,
        value: [0.25, 0.5, 0.75, 1.0],
        depth_value: 0.5,
        stencil_value: 3,
    }
}

fn whole(id: RenderTextureId) -> Attachment {
    Attachment {
        texture: ids::render_texture(id),
        mip: 0,
    }
}

fn temporary_desc(size: u32) -> WorldRenderTexture {
    WorldRenderTexture {
        width: size,
        height: size,
        format: 8,
        depth_bits: 32,
        mip_count: 1,
        srgb: false,
        dimension: 2,
        volume_depth: 1,
        sampler: WorldSampler::default(),
        generation: 0,
    }
}

fn camera(world: &mut World) -> ComponentHandle {
    let object = object(world, 0, Vec3::ZERO, Quat::IDENTITY);
    world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap()
}

fn clears(adapted: &Adapted) -> Vec<(Attachments, Clear)> {
    adapted
        .frame
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Clear { target, clear } => Some((*target, *clear)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_clear_sets_the_color_depth_and_stencil_it_names_on_the_bound_targets() {
    let mut world = world();
    let a = render_texture(&mut world, [8, 8]);
    record(&mut world, target(&[TargetRef::Render(a)], None));
    record(&mut world, clear(true, true, true));
    record(&mut world, clear(false, true, false));
    let adapted = run(&mut world);
    let bound = Attachments {
        color: Some(whole(a)),
        depth: Some(whole(a)),
    };
    assert_eq!(
        clears(&adapted),
        [
            (
                bound,
                Clear {
                    color: Some([0.25, 0.5, 0.75, 1.0]),
                    depth: Some(0.5),
                    stencil: Some(3),
                }
            ),
            (
                bound,
                Clear {
                    color: None,
                    depth: Some(0.5),
                    stencil: None,
                }
            ),
        ]
    );
    assert_eq!(adapted.report.built_by_kind["SetRenderTarget"], 1);
    assert_eq!(adapted.report.built_by_kind["ClearRenderTarget"], 2);
    assert_eq!(adapted.frame.render_textures.len(), 1);
}

#[test]
fn a_clear_that_names_nothing_makes_no_record() {
    let mut world = world();
    let a = render_texture(&mut world, [8, 8]);
    record(&mut world, target(&[TargetRef::Render(a)], None));
    record(&mut world, clear(false, false, false));
    let adapted = run(&mut world);
    assert!(adapted.frame.records.is_empty());
    assert!(adapted.report.skipped.is_empty());
}

#[test]
fn a_clear_with_no_target_bound_is_skipped_by_reason() {
    let mut world = world();
    record(&mut world, clear(true, false, false));
    let adapted = run(&mut world);
    assert!(adapted.frame.records.is_empty());
    assert_eq!(adapted.report.skipped.get(&Skip::UnboundTarget), Some(&1));
    assert_eq!(
        adapted.report.skipped_by_kind["ClearRenderTarget: UnboundTarget"],
        1
    );
}

#[test]
fn a_target_of_one_color_texture_and_another_depth_texture_binds_both() {
    let mut world = world();
    let (color, depth) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    record(
        &mut world,
        target(&[TargetRef::Render(color)], Some(TargetRef::Render(depth))),
    );
    record(&mut world, clear(true, true, false));
    let adapted = run(&mut world);
    assert_eq!(
        clears(&adapted)[0].0,
        Attachments {
            color: Some(whole(color)),
            depth: Some(whole(depth)),
        }
    );
    assert_eq!(adapted.frame.render_textures.len(), 2);
}

#[test]
fn a_target_with_a_depth_texture_alone_has_no_color() {
    let mut world = world();
    let depth = render_texture(&mut world, [8, 8]);
    record(&mut world, target(&[], Some(TargetRef::Render(depth))));
    record(&mut world, clear(true, true, false));
    let adapted = run(&mut world);
    assert_eq!(
        clears(&adapted)[0].0,
        Attachments {
            color: None,
            depth: Some(whole(depth)),
        }
    );
}

#[test]
fn a_target_names_the_mip_it_draws_into_and_a_missing_mip_is_skipped() {
    let mut world = world();
    let a = world
        .create_render_texture(WorldRenderTexture {
            mip_count: 3,
            ..temporary_desc(8)
        })
        .unwrap();
    let at_mip = |mip| OffscreenWork::SetRenderTarget {
        color: vec![TargetRef::Render(a)],
        depth: None,
        mip,
        face: -1,
        slice: -1,
    };
    record(&mut world, at_mip(2));
    record(&mut world, clear(true, false, false));
    record(&mut world, at_mip(3));
    record(&mut world, clear(true, false, false));
    let adapted = run(&mut world);
    let level_two = Some(Attachment {
        texture: ids::render_texture(a),
        mip: 2,
    });
    // The second target is refused, so the second clear goes to the mip bound before it.
    assert_eq!(clears(&adapted).len(), 2);
    for (target, _) in clears(&adapted) {
        assert_eq!(
            target,
            Attachments {
                color: level_two,
                depth: level_two,
            }
        );
    }
    assert_eq!(adapted.report.skipped.get(&Skip::NoSuchMip), Some(&1));
}

#[test]
fn a_target_that_names_a_cube_face_or_slice_past_the_first_is_skipped() {
    let mut world = world();
    let a = render_texture(&mut world, [8, 8]);
    record(
        &mut world,
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(a)],
            depth: None,
            mip: 0,
            face: 2,
            slice: -1,
        },
    );
    record(
        &mut world,
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(a)],
            depth: None,
            mip: 0,
            face: -1,
            slice: 1,
        },
    );
    let adapted = run(&mut world);
    assert_eq!(
        adapted.report.skipped.get(&Skip::UnsupportedSlice),
        Some(&2)
    );
}

#[test]
fn a_refused_mesh_draw_is_counted_by_its_reason_and_leaves_the_running_state_alone() {
    let mut world = world();
    let a = render_texture(&mut world, [8, 8]);
    let shader = world
        .create_shader(WorldShader {
            name: "Hidden/Portrait".into(),
            source: None,
        })
        .unwrap();
    let material = world.create_material(shader).unwrap();
    record(&mut world, target(&[TargetRef::Render(a)], None));
    for index in 0..2 {
        record(
            &mut world,
            OffscreenWork::DrawMesh {
                // No mesh of the snapshot is held by the host.
                mesh: (1 << 41) + index,
                matrix: Mat4::IDENTITY.to_cols_array(),
                material,
                submesh: 0,
                pass: -1,
                block: None,
            },
        );
    }
    record(&mut world, clear(true, false, false));
    let adapted = run(&mut world);
    assert_eq!(adapted.report.skipped.get(&Skip::MeshUnavailable), Some(&2));
    assert_eq!(
        adapted
            .report
            .skipped_by_kind
            .get("DrawMesh: MeshUnavailable"),
        Some(&2)
    );
    assert_eq!(
        clears(&adapted).len(),
        1,
        "the clear after the draws still lands"
    );
    assert_eq!(adapted.report.built_by_kind.get("DrawMesh"), None);
}

#[test]
fn a_temporary_render_texture_is_made_for_the_target_that_names_it() {
    let mut world = world();
    record(
        &mut world,
        OffscreenWork::AllocateTemporary {
            name_id: 7,
            desc: temporary_desc(4),
        },
    );
    record(&mut world, target(&[TargetRef::Temporary(7)], None));
    record(&mut world, clear(true, true, false));
    let adapted = run(&mut world);
    let id = ids::temporary(7, 0);
    let whole = Some(Attachment {
        texture: id,
        mip: 0,
    });
    assert_eq!(
        clears(&adapted)[0].0,
        Attachments {
            color: whole,
            depth: whole,
        }
    );
    assert_eq!(
        adapted.frame.render_textures,
        [RenderTextureSpec {
            id,
            size: [4, 4],
            dimension: RenderDimension::D2,
            graphics_format: 8,
            depth_bits: 32,
            mips: 1,
            srgb: false,
            generation: 0,
        }]
    );
    assert!(ids::is_temporary(id));
    assert_eq!(ids::render_texture_of(id), None);
}

#[test]
fn a_name_allocated_again_is_another_texture_and_a_released_one_cannot_be_targeted() {
    let mut world = world();
    let allocate = |size| OffscreenWork::AllocateTemporary {
        name_id: 7,
        desc: temporary_desc(size),
    };
    record(&mut world, allocate(4));
    record(&mut world, target(&[TargetRef::Temporary(7)], None));
    record(&mut world, clear(true, false, false));
    record(&mut world, OffscreenWork::ReleaseTemporary { name_id: 7 });
    record(&mut world, allocate(2));
    record(&mut world, target(&[TargetRef::Temporary(7)], None));
    record(&mut world, clear(true, false, false));
    record(&mut world, OffscreenWork::ReleaseTemporary { name_id: 7 });
    record(&mut world, target(&[TargetRef::Temporary(7)], None));
    record(&mut world, OffscreenWork::ReleaseTemporary { name_id: 9 });
    let adapted = run(&mut world);
    let drawn: Vec<_> = clears(&adapted)
        .iter()
        .map(|(target, _)| target.color.unwrap().texture)
        .collect();
    assert_eq!(drawn, [ids::temporary(7, 0), ids::temporary(7, 1)]);
    let sizes: BTreeMap<_, _> = adapted
        .frame
        .render_textures
        .iter()
        .map(|spec| (spec.id, spec.size))
        .collect();
    assert_eq!(sizes[&ids::temporary(7, 0)], [4, 4]);
    assert_eq!(sizes[&ids::temporary(7, 1)], [2, 2]);
    assert_eq!(
        adapted.report.skipped.get(&Skip::UnknownTemporary),
        Some(&2)
    );
}

#[test]
fn the_temporaries_a_work_list_allocates_are_named_before_it_is_adapted() {
    let allocate = |name_id| OffscreenWork::AllocateTemporary {
        name_id,
        desc: temporary_desc(4),
    };
    let work = [
        allocate(7),
        OffscreenWork::ReleaseTemporary { name_id: 7 },
        allocate(7),
        allocate(-3),
    ];
    let expected: BTreeSet<_> = [
        ids::temporary(7, 0),
        ids::temporary(7, 1),
        ids::temporary(-3, 0),
    ]
    .into();
    assert_eq!(super::super::temporary_ids(&work), expected);
}

#[test]
fn globals_set_by_commands_reach_the_records_after_them_and_not_the_ones_before() {
    let mut world = world();
    let a = render_texture(&mut world, [8, 8]);
    let map = render_texture(&mut world, [4, 4]);
    record(&mut world, target(&[TargetRef::Render(a)], None));
    record(&mut world, clear(true, false, false));
    record(
        &mut world,
        OffscreenWork::SetGlobal(WorldValue::Float("_Scale".to_owned(), 2.0)),
    );
    record(
        &mut world,
        OffscreenWork::SetGlobalKeyword {
            name: "FOG".to_owned(),
            enabled: true,
        },
    );
    record(
        &mut world,
        OffscreenWork::SetGlobal(WorldValue::Texture(
            "_Map".to_owned(),
            texture_slot(TextureRef::Render(map)),
        )),
    );
    record(&mut world, clear(true, false, false));
    record(
        &mut world,
        OffscreenWork::SetGlobal(WorldValue::Float("_Scale".to_owned(), 5.0)),
    );
    record(
        &mut world,
        OffscreenWork::SetGlobalKeyword {
            name: "FOG".to_owned(),
            enabled: false,
        },
    );
    record(&mut world, clear(true, false, false));
    // A change after the last record that draws or clears has no record to carry it.
    record(
        &mut world,
        OffscreenWork::SetGlobal(WorldValue::Float("_Late".to_owned(), 1.0)),
    );
    let adapted = run(&mut world);
    let kinds: Vec<_> = adapted
        .frame
        .records
        .iter()
        .map(|record| match record {
            Record::Clear { .. } => "clear",
            Record::Globals(_) => "globals",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["clear", "globals", "clear", "globals", "clear"],
        "the first clear precedes every change"
    );
    let globals: Vec<_> = adapted
        .frame
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Globals(state) => Some(state.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        globals[0].values.values.get("_Scale"),
        Some(&PropertyValue::Float(2.0))
    );
    assert_eq!(globals[0].keywords, ["FOG"]);
    assert_eq!(
        globals[0].values.textures["_Map"].texture,
        ids::render_texture(map)
    );
    assert_eq!(
        globals[1].values.values.get("_Scale"),
        Some(&PropertyValue::Float(5.0))
    );
    assert!(globals[1].keywords.is_empty());
    assert_eq!(
        globals[1].values.textures["_Map"].texture,
        ids::render_texture(map),
        "a global stays until it is set again"
    );
    assert_eq!(
        adapted.report.built, 3,
        "the globals are not counted as built"
    );
    let specs: Vec<_> = adapted
        .frame
        .render_textures
        .iter()
        .map(|s| s.size)
        .collect();
    assert!(
        specs.contains(&[4, 4]),
        "the global texture is in the frame"
    );
}

#[test]
fn a_global_texture_set_to_nothing_is_taken_away() {
    let mut world = world();
    let a = render_texture(&mut world, [8, 8]);
    let map = render_texture(&mut world, [4, 4]);
    record(&mut world, target(&[TargetRef::Render(a)], None));
    for texture in [TextureRef::Render(map), TextureRef::None] {
        record(
            &mut world,
            OffscreenWork::SetGlobal(WorldValue::Texture(
                "_Map".to_owned(),
                texture_slot(texture),
            )),
        );
        record(&mut world, clear(true, false, false));
    }
    let adapted = run(&mut world);
    let held: Vec<_> = adapted
        .frame
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Globals(state) => Some(state.values.textures.contains_key("_Map")),
            _ => None,
        })
        .collect();
    assert_eq!(held, [true, false]);
}

#[test]
fn a_camera_setup_binds_its_texture_and_the_camera_target_names_it_after() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [4, 4]),
    );
    let camera = camera(&mut world);
    record(
        &mut world,
        OffscreenWork::SetupCamera {
            camera,
            view: Mat4::IDENTITY.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target: TargetRef::Render(a),
        },
    );
    record(&mut world, clear(true, false, false));
    record(&mut world, target(&[TargetRef::Render(b)], None));
    record(&mut world, clear(true, false, false));
    record(&mut world, target(&[TargetRef::CameraTarget], None));
    record(&mut world, clear(true, false, false));
    let adapted = run(&mut world);
    let drawn: Vec<_> = clears(&adapted)
        .iter()
        .map(|(target, _)| target.color.unwrap().texture)
        .collect();
    assert_eq!(
        drawn,
        [
            ids::render_texture(a),
            ids::render_texture(b),
            ids::render_texture(a)
        ]
    );
}

#[test]
fn the_camera_target_of_a_camera_that_has_no_texture_is_skipped() {
    let mut world = world();
    let camera = camera(&mut world);
    record(
        &mut world,
        OffscreenWork::SetupCamera {
            camera,
            view: Mat4::IDENTITY.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target: TargetRef::CameraTarget,
        },
    );
    record(&mut world, target(&[TargetRef::CameraTarget], None));
    record(&mut world, clear(true, false, false));
    let adapted = run(&mut world);
    assert_eq!(adapted.report.skipped.get(&Skip::UnboundTarget), Some(&2));
    assert!(adapted.frame.records.is_empty());
}
