//! Command buffer records through the ABI: each call validates and forwards one record, and the
//! frame's work holds the records in call order.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_commands::*, graphics_offscreen::*, graphics_properties::*,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, RenderTextureId, TargetRef, TextureRef, TextureSlot,
};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn text(name: &str) -> (*const u8, u32) {
    (name.as_ptr(), u32::try_from(name.len()).unwrap())
}

fn description() -> DisunityRenderTexture {
    DisunityRenderTexture {
        width: 64,
        height: 32,
        format: 8,
        depth_bits: 24,
        mip_count: 3,
        srgb: 1,
        filter: 1,
        aniso_level: 1,
        dimension: 2,
        volume_depth: 1,
        ..Default::default()
    }
}

fn render_texture(world: u64) -> u64 {
    let desc = description();
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_render_texture_create(world, &desc, &mut id).code,
            0
        )
    };
    id
}

fn target(kind: u32, name_id: i32, render: u64) -> DisunityTargetRef {
    DisunityTargetRef {
        kind,
        name_id,
        render,
    }
}

fn rejected(code: i32) {
    assert_ne!(code, 0);
}

/// Records one of every command and returns the color value it used.
fn record_every_command(world: u64, id: u64) -> [f32; 4] {
    let colors = [target(0, 0, id), target(1, -77, 0), target(2, 0, 0)];
    let depth = target(1, 9, 0);
    let value = [0.25, 0.5, 0.75, 1.0];
    let slot = DisunityTextureSlot {
        kind: 2,
        texture: id,
        scale_x: 2.0,
        scale_y: 3.0,
        offset_x: 0.5,
        offset_y: 0.25,
    };
    let desc = description();
    let (tname, tlen) = text("_Tex");
    unsafe {
        let results = [
            disunity_offscreen_allocate_temporary(world, -77, &desc),
            disunity_offscreen_set_render_target(world, colors.as_ptr(), 3, &depth, 2, 4, 1),
            disunity_offscreen_clear_render_target(world, 1 | 4, value.as_ptr(), 0.5, 9),
            disunity_offscreen_set_global_texture(world, tname, tlen, &slot),
            disunity_offscreen_release_temporary(world, -77),
        ];
        for result in results {
            assert_eq!(result.code, 0);
        }
    }
    value
}

#[test]
fn every_command_call_is_recorded_in_order_with_its_detail() {
    let world = new_world();
    let id = render_texture(world);
    let value = record_every_command(world, id);
    let details = snapshot(world).work;
    assert_eq!(details.len(), 5);
    let render = RenderTextureId::from_bits(id);
    assert!(matches!(
        details[0],
        OffscreenWork::AllocateTemporary { name_id: -77, desc } if desc.width == 64 && desc.depth_bits == 24 && desc.srgb
    ));
    assert_eq!(
        details[1],
        OffscreenWork::SetRenderTarget {
            color: vec![
                TargetRef::Render(render),
                TargetRef::Temporary(-77),
                TargetRef::CameraTarget
            ],
            depth: Some(TargetRef::Temporary(9)),
            mip: 2,
            face: 4,
            slice: 1,
        }
    );
    assert_eq!(
        details[2],
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: true,
            value,
            depth_value: 0.5,
            stencil_value: 9,
        }
    );
    assert_global_details(&details[3], render);
    assert_eq!(details[4], OffscreenWork::ReleaseTemporary { name_id: -77 });
    assert!(snapshot(world).work.is_empty());
}

fn assert_global_details(detail: &OffscreenWork, render: RenderTextureId) {
    let slot = TextureSlot {
        texture: TextureRef::Render(render),
        scale: [2.0, 3.0],
        offset: [0.5, 0.25],
    };
    assert_eq!(
        *detail,
        OffscreenWork::SetGlobal(PropertyValue::Texture("_Tex".into(), slot))
    );
}

/// Binds `colors` and an optional `depth` at cube face `face`.
fn bind(
    world: u64,
    colors: &[DisunityTargetRef],
    depth: Option<&DisunityTargetRef>,
    face: i32,
) -> i32 {
    let list = if colors.is_empty() {
        std::ptr::null()
    } else {
        colors.as_ptr()
    };
    let depth = depth.map_or(std::ptr::null(), std::ptr::from_ref);
    let count = u32::try_from(colors.len()).unwrap();
    unsafe { disunity_offscreen_set_render_target(world, list, count, depth, 0, face, -1) }.code
}

#[test]
fn a_target_list_that_is_malformed_records_nothing() {
    let world = new_world();
    let id = render_texture(world);
    let dead = target(0, 0, id + (1 << 32));
    let ok = target(0, 0, id);
    let nine = [target(3, 0, 0); 9];
    rejected(
        unsafe {
            disunity_offscreen_set_render_target(
                world,
                std::ptr::null(),
                1,
                std::ptr::null(),
                0,
                -1,
                -1,
            )
        }
        .code,
    );
    rejected(bind(world, &nine, None, -1));
    rejected(bind(world, &[dead], None, -1));
    rejected(bind(world, &[ok], Some(&dead), -1));
    rejected(bind(world, &[ok], None, 6));
    rejected(bind(world, &[], None, -1));
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn a_target_with_fields_its_kind_does_not_use_records_nothing() {
    let world = new_world();
    let id = render_texture(world);
    let ok = target(0, 0, id);
    let refused = [
        target(9, 0, 0),
        target(2, 5, 0),
        target(0, 0, 0),
        target(1, 3, id),
        target(0, 3, id),
        target(3, 0, 1),
    ];
    for bad in refused {
        rejected(bind(world, &[bad], None, -1));
        rejected(bind(world, &[ok], Some(&bad), -1));
    }
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn a_clear_call_with_a_bad_argument_records_nothing() {
    let world = new_world();
    let value = [0.0_f32; 4];
    let nan = [0.0, f32::NAN, 0.0, 0.0];
    unsafe {
        rejected(disunity_offscreen_clear_render_target(world, 1, std::ptr::null(), 0.0, 0).code);
        rejected(disunity_offscreen_clear_render_target(world, 8, value.as_ptr(), 0.0, 0).code);
        rejected(disunity_offscreen_clear_render_target(world, 1, nan.as_ptr(), 0.0, 0).code);
        rejected(
            disunity_offscreen_clear_render_target(world, 1, value.as_ptr(), f32::INFINITY, 0).code,
        );
    }
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn a_global_texture_call_with_a_bad_argument_records_nothing() {
    let world = new_world();
    let (name, length) = text("_G");
    let dead_slot = DisunityTextureSlot {
        kind: 2,
        texture: 77,
        scale_x: 1.0,
        scale_y: 1.0,
        ..Default::default()
    };
    let no_slot: *const DisunityTextureSlot = std::ptr::null();
    unsafe {
        rejected(disunity_offscreen_set_global_texture(world, name, length, no_slot).code);
        rejected(disunity_offscreen_set_global_texture(world, name, length, &dead_slot).code);
        let live = DisunityTextureSlot {
            kind: 0,
            scale_x: 1.0,
            scale_y: 1.0,
            ..Default::default()
        };
        rejected(disunity_offscreen_set_global_texture(world, std::ptr::null(), 0, &live).code);
        rejected(disunity_offscreen_set_global_texture(world, std::ptr::null(), 2, &live).code);
        rejected(disunity_offscreen_set_global_texture(world, name, 0, &live).code);
    }
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn temporary_calls_with_bad_arguments_record_nothing() {
    let world = new_world();
    let wide = DisunityRenderTexture {
        width: 0,
        ..description()
    };
    let srgb = DisunityRenderTexture {
        srgb: 2,
        ..description()
    };
    unsafe {
        rejected(disunity_offscreen_allocate_temporary(world, 1, std::ptr::null()).code);
        rejected(disunity_offscreen_allocate_temporary(world, 1, &wide).code);
        rejected(disunity_offscreen_allocate_temporary(world, 1, &srgb).code);
    }
    rejected(disunity_offscreen_release_temporary(world + 99, 1).code);
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn commands_cannot_be_recorded_through_the_flat_record() {
    let world = new_world();
    for kind in 4..=9 {
        let record = DisunityOffscreenWork {
            kind,
            ..Default::default()
        };
        rejected(unsafe { disunity_offscreen_record(world, &record) }.code);
    }
    assert!(snapshot(world).work.is_empty());
}
