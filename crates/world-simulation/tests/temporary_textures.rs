//! Temporary render textures as the texture recorded work reads and the target a blit or a copy
//! writes, and their refusal everywhere else.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode_work, encode_work};
use worldrunner_world_simulation::graphics::{
    GraphicsError, MaterialEdit, OffscreenWork, PropertyValue, TargetRef, TextureRef, TextureSlot,
    WorldRenderTexture, WorldSampler, WorldShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

const NAME_ID: i32 = -7;

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn texture(mip_count: u32) -> WorldRenderTexture {
    WorldRenderTexture {
        width: 16,
        height: 16,
        format: 8,
        depth_bits: 0,
        mip_count,
        srgb: false,
        dimension: 2,
        volume_depth: 1,
        auto_generate_mips: false,
        sampler: WorldSampler::default(),
        generation: 0,
    }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn temporary_value() -> PropertyValue {
    PropertyValue::Texture(
        "_AO_CurrDepthSource".into(),
        TextureSlot {
            texture: TextureRef::Temporary(NAME_ID),
            ..TextureSlot::default()
        },
    )
}

/// The records the game's ambient occlusion pass makes for one mip: a blit of a temporary into
/// itself, a copy of it into a mip of a render texture and a global that names it.
fn occlusion_mip(world: &mut World) -> Vec<OffscreenWork> {
    let mipmap = world.create_render_texture(texture(3)).unwrap();
    vec![
        OffscreenWork::AllocateTemporary {
            name_id: NAME_ID,
            desc: texture(1),
        },
        OffscreenWork::Blit {
            source: TextureRef::Temporary(NAME_ID),
            target: TargetRef::Temporary(NAME_ID),
            material: None,
            pass: 35,
        },
        OffscreenWork::CopyTexture {
            source: TextureRef::Temporary(NAME_ID),
            source_element: 0,
            source_mip: 0,
            target: TargetRef::Render(mipmap),
            target_element: 0,
            target_mip: 2,
        },
        OffscreenWork::SetGlobal(temporary_value()),
        OffscreenWork::ReleaseTemporary { name_id: NAME_ID },
    ]
}

#[test]
fn recorded_work_reads_and_writes_a_temporary_in_call_order() {
    let mut world = world();
    let work = occlusion_mip(&mut world);
    for record in &work {
        world.record_offscreen(record.clone()).unwrap();
    }
    assert_eq!(world.take_offscreen().0, work);
}

#[test]
fn a_temporary_texture_and_target_survive_the_wire() {
    let mut world = world();
    for record in occlusion_mip(&mut world) {
        let bytes = encode_work(&record).unwrap();
        assert_eq!(decode_work(&bytes).unwrap(), record);
    }
}

#[test]
fn a_temporary_is_refused_outside_recorded_work() {
    let mut world = world();
    let shader = world
        .create_shader(WorldShader {
            name: "Hidden/Amplify Occlusion/Occlusion".into(),
            source: None,
        })
        .unwrap();
    let material = world.create_material(shader).unwrap();
    assert_eq!(
        graphics_error(world.edit_material(material, MaterialEdit::Property(temporary_value()))),
        GraphicsError::InvalidTexture
    );
    assert_eq!(
        graphics_error(world.set_global_property(temporary_value())),
        GraphicsError::InvalidTexture
    );
    assert_eq!(
        graphics_error(world.check_property(&temporary_value())),
        GraphicsError::InvalidTexture
    );
    assert!(
        world
            .global_shader_state()
            .properties
            .texture("_AO_CurrDepthSource")
            .is_none()
    );
}

#[test]
fn a_blit_or_copy_writes_only_a_render_texture_or_a_temporary() {
    let mut world = world();
    for target in [TargetRef::CameraTarget, TargetRef::None] {
        let blit = OffscreenWork::Blit {
            source: TextureRef::Temporary(NAME_ID),
            target,
            material: None,
            pass: -1,
        };
        let copy = OffscreenWork::CopyTexture {
            source: TextureRef::Temporary(NAME_ID),
            source_element: 0,
            source_mip: 0,
            target,
            target_element: 0,
            target_mip: 0,
        };
        for record in [blit, copy] {
            assert_eq!(
                graphics_error(world.record_offscreen(record)),
                GraphicsError::InvalidOffscreen
            );
        }
    }
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_copy_into_a_render_texture_still_needs_the_mip() {
    let mut world = world();
    let target = world.create_render_texture(texture(2)).unwrap();
    let copy = |target_mip| OffscreenWork::CopyTexture {
        source: TextureRef::Temporary(NAME_ID),
        source_element: 0,
        source_mip: 0,
        target: TargetRef::Render(target),
        target_element: 0,
        target_mip,
    };
    world.record_offscreen(copy(1)).unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(copy(2))),
        GraphicsError::InvalidOffscreen
    );
}

#[test]
fn a_blit_names_the_render_texture_it_writes_but_not_a_temporary() {
    let mut world = world();
    let held = world.create_render_texture(texture(1)).unwrap();
    let blit = |target| OffscreenWork::Blit {
        source: TextureRef::Temporary(NAME_ID),
        target,
        material: None,
        pass: -1,
    };
    assert!(blit(TargetRef::Render(held)).names_render_texture(held));
    assert!(!blit(TargetRef::Temporary(NAME_ID)).names_render_texture(held));
}
