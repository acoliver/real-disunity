//! Temporary render textures through the ABI: texture kind 4 and target kind 1 name one by its
//! property name id in recorded work, and every other place a texture is written refuses it.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_commands::*, graphics_offscreen::*, graphics_properties::*,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, TargetRef, TextureRef,
};

const SCOPE_MATERIAL: u32 = 0;
const TEXTURE_TEMPORARY: u32 = 4;
const TARGET_TEMPORARY: u32 = 1;
const NAME_ID: i32 = -7;

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn text(name: &str) -> (*const u8, u32) {
    (name.as_ptr(), u32::try_from(name.len()).unwrap())
}

fn name_bits() -> u64 {
    u64::from(NAME_ID.cast_unsigned())
}

fn render_texture(world: u64) -> u64 {
    let desc = DisunityRenderTexture {
        width: 16,
        height: 16,
        format: 8,
        mip_count: 3,
        filter: 1,
        aniso_level: 1,
        dimension: 2,
        volume_depth: 1,
        ..Default::default()
    };
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_render_texture_create(world, &desc, &mut id).code,
            0
        )
    };
    id
}

fn material(world: u64) -> u64 {
    let (bytes, length) = text("Hidden/Amplify Occlusion/Occlusion");
    let (mut shader, mut id) = (0, 0);
    unsafe {
        assert_eq!(
            disunity_shader_create(world, bytes, length, &mut shader).code,
            0
        );
        assert_eq!(disunity_material_create(world, shader, &mut id).code, 0);
    }
    id
}

fn temporary_slot() -> DisunityTextureSlot {
    DisunityTextureSlot {
        kind: TEXTURE_TEMPORARY,
        texture: name_bits(),
        scale_x: 1.0,
        scale_y: 1.0,
        ..DisunityTextureSlot::default()
    }
}

#[test]
fn a_blit_and_a_copy_read_and_write_a_temporary_by_its_name_id() {
    let world = new_world();
    let held = render_texture(world);
    let records = [
        DisunityOffscreenWork {
            kind: 1,
            source_kind: TEXTURE_TEMPORARY,
            source: name_bits(),
            target_kind: TARGET_TEMPORARY,
            target: name_bits(),
            pass: 35,
            ..DisunityOffscreenWork::default()
        },
        DisunityOffscreenWork {
            kind: 2,
            source_kind: TEXTURE_TEMPORARY,
            source: name_bits(),
            target: held,
            target_mip: 2,
            ..DisunityOffscreenWork::default()
        },
        DisunityOffscreenWork {
            kind: 2,
            source_kind: 2,
            source: held,
            target_kind: TARGET_TEMPORARY,
            target: name_bits(),
            ..DisunityOffscreenWork::default()
        },
    ];
    for record in &records {
        unsafe { assert_eq!(disunity_offscreen_record(world, record).code, 0) };
    }
    let held = RenderTextureId::from_bits(held);
    let copy = |source, target, target_mip| OffscreenWork::CopyTexture {
        source,
        source_element: 0,
        source_mip: 0,
        target,
        target_element: 0,
        target_mip,
    };
    assert_eq!(
        snapshot(world).work,
        [
            OffscreenWork::Blit {
                source: TextureRef::Temporary(NAME_ID),
                target: TargetRef::Temporary(NAME_ID),
                material: None,
                pass: 35,
            },
            copy(TextureRef::Temporary(NAME_ID), TargetRef::Render(held), 2),
            copy(TextureRef::Render(held), TargetRef::Temporary(NAME_ID), 0),
        ]
    );
}

#[test]
fn a_target_kind_or_name_id_the_world_cannot_hold_is_refused() {
    let world = new_world();
    let blit = |source_kind, source, target_kind, target| DisunityOffscreenWork {
        kind: 1,
        source_kind,
        source,
        target_kind,
        target,
        pass: -1,
        ..DisunityOffscreenWork::default()
    };
    let bad = [
        blit(TEXTURE_TEMPORARY, name_bits(), 2, name_bits()),
        blit(TEXTURE_TEMPORARY, 1 << 32, TARGET_TEMPORARY, name_bits()),
        blit(TEXTURE_TEMPORARY, name_bits(), TARGET_TEMPORARY, 1 << 32),
    ];
    for record in bad {
        let result = unsafe { disunity_offscreen_record(world, &record) };
        assert_ne!(result.code, 0, "{record:?}");
    }
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn a_global_set_by_recorded_work_takes_a_temporary_and_a_material_does_not() {
    let world = new_world();
    let material = material(world);
    let (name, length) = text("_AO_CurrDepthSource");
    let slot = temporary_slot();
    unsafe {
        let stored =
            disunity_shader_texture_write(world, SCOPE_MATERIAL, material, name, length, &slot);
        assert_ne!(stored.code, 0);
        let recorded = disunity_offscreen_set_global_texture(world, name, length, &slot);
        assert_eq!(recorded.code, 0);
    }
    assert_eq!(snapshot(world).work.len(), 1);
}
