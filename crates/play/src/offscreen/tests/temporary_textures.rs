//! Temporary render textures as the texture a blit, a copy or a global reads and as the target a
//! blit or a copy writes. The worlds are synthetic; each test predicts the records.

use worldrunner_render_program::ids::{MaterialId, TextureId};
use worldrunner_render_program::unity::offscreen::frame::{Record, RenderTextureSpec};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue as WorldValue, TargetRef, TextureRef, WorldRenderTexture,
    WorldSampler,
};

use super::super::frame::{Adapted, Settings, Skip, adapt};
use super::super::ids;
use super::support::{available, render_texture, snapshot, texture_slot, world};

const NAME_ID: i32 = 7;

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

fn allocate(world: &mut World, mip_count: u32) {
    record(
        world,
        OffscreenWork::AllocateTemporary {
            name_id: NAME_ID,
            desc: WorldRenderTexture {
                width: 8,
                height: 4,
                format: 8,
                depth_bits: 0,
                mip_count,
                srgb: false,
                dimension: 2,
                volume_depth: 1,
                auto_generate_mips: false,
                sampler: WorldSampler::default(),
                generation: 0,
            },
        },
    );
}

fn blit(source: TextureRef, target: TargetRef) -> OffscreenWork {
    OffscreenWork::Blit {
        source,
        target,
        material: None,
        pass: -1,
    }
}

/// What a blit or a copy record names, which the tests compare.
#[derive(Debug, PartialEq)]
enum Named {
    Blit {
        source: TextureId,
        target: TextureId,
        material: Option<MaterialId>,
        pass: usize,
    },
    Copy {
        source: TextureId,
        source_mip: u32,
        target: TextureId,
        target_mip: u32,
    },
    Other,
}

fn named(adapted: &Adapted) -> Vec<Named> {
    adapted
        .frame
        .records
        .iter()
        .map(|record| match *record {
            Record::Blit {
                source,
                target,
                material,
                pass,
            } => Named::Blit {
                source,
                target,
                material,
                pass,
            },
            Record::CopyTexture {
                source,
                source_element: 0,
                source_mip,
                target,
                target_element: 0,
                target_mip,
            } => Named::Copy {
                source,
                source_mip,
                target,
                target_mip,
            },
            _ => Named::Other,
        })
        .collect()
}

fn spec(adapted: &Adapted, id: TextureId) -> &RenderTextureSpec {
    adapted
        .frame
        .render_textures
        .iter()
        .find(|spec| spec.id == id)
        .unwrap_or_else(|| panic!("no texture {id:?} in the frame"))
}

#[test]
fn a_temporary_is_blitted_and_copied_into_a_render_texture() {
    let mut world = world();
    let held = render_texture(&mut world, [8, 4]);
    allocate(&mut world, 1);
    record(
        &mut world,
        blit(TextureRef::Temporary(NAME_ID), TargetRef::Render(held)),
    );
    record(
        &mut world,
        OffscreenWork::CopyTexture {
            source: TextureRef::Temporary(NAME_ID),
            source_element: 0,
            source_mip: 0,
            target: TargetRef::Render(held),
            target_element: 0,
            target_mip: 0,
        },
    );
    let adapted = run(&mut world);
    let temporary = ids::temporary(NAME_ID, 0);
    let target = ids::render_texture(held);
    assert_eq!(
        named(&adapted),
        [
            Named::Blit {
                source: temporary,
                target,
                material: None,
                pass: 0,
            },
            Named::Copy {
                source: temporary,
                source_mip: 0,
                target,
                target_mip: 0,
            },
        ]
    );
    assert_eq!(spec(&adapted, temporary).size, [8, 4]);
}

#[test]
fn a_blit_into_a_temporary_writes_it() {
    let mut world = world();
    let held = render_texture(&mut world, [8, 4]);
    allocate(&mut world, 1);
    record(
        &mut world,
        blit(TextureRef::Render(held), TargetRef::Temporary(NAME_ID)),
    );
    let adapted = run(&mut world);
    assert_eq!(
        named(&adapted),
        [Named::Blit {
            source: ids::render_texture(held),
            target: ids::temporary(NAME_ID, 0),
            material: None,
            pass: 0,
        }]
    );
}

#[test]
fn a_blit_of_a_temporary_into_itself_names_it_at_both_ends() {
    let mut world = world();
    allocate(&mut world, 2);
    record(
        &mut world,
        blit(
            TextureRef::Temporary(NAME_ID),
            TargetRef::Temporary(NAME_ID),
        ),
    );
    let adapted = run(&mut world);
    let temporary = ids::temporary(NAME_ID, 0);
    assert_eq!(
        named(&adapted),
        [Named::Blit {
            source: temporary,
            target: temporary,
            material: None,
            pass: 0,
        }]
    );
    assert_eq!(adapted.frame.render_textures.len(), 1);
}

#[test]
fn a_global_set_by_recorded_work_names_the_temporary_alive_at_that_point() {
    let mut world = world();
    let held = render_texture(&mut world, [8, 4]);
    allocate(&mut world, 1);
    record(
        &mut world,
        OffscreenWork::SetGlobal(WorldValue::Texture(
            "_AO_CurrDepthSource".to_owned(),
            texture_slot(TextureRef::Temporary(NAME_ID)),
        )),
    );
    record(
        &mut world,
        blit(TextureRef::Render(held), TargetRef::Temporary(NAME_ID)),
    );
    let adapted = run(&mut world);
    let named: Vec<_> = adapted
        .frame
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Globals(state) => state
                .values
                .textures
                .get("_AO_CurrDepthSource")
                .map(|input| input.texture),
            _ => None,
        })
        .collect();
    assert_eq!(named, [ids::temporary(NAME_ID, 0)]);
}

#[test]
fn a_temporary_that_is_not_allocated_is_skipped_by_reason() {
    let mut world = world();
    let held = render_texture(&mut world, [8, 4]);
    record(
        &mut world,
        blit(TextureRef::Temporary(NAME_ID), TargetRef::Render(held)),
    );
    record(
        &mut world,
        blit(TextureRef::Render(held), TargetRef::Temporary(NAME_ID)),
    );
    allocate(&mut world, 1);
    record(
        &mut world,
        OffscreenWork::ReleaseTemporary { name_id: NAME_ID },
    );
    record(
        &mut world,
        OffscreenWork::SetGlobal(WorldValue::Texture(
            "_Released".to_owned(),
            texture_slot(TextureRef::Temporary(NAME_ID)),
        )),
    );
    let adapted = run(&mut world);
    assert!(adapted.frame.records.is_empty());
    assert_eq!(
        adapted.report.skipped.get(&Skip::UnknownTemporary),
        Some(&3)
    );
}
