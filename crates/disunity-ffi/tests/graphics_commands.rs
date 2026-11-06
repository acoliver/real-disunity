//! Command buffer records through the ABI: each call validates and forwards one record, and a
//! take hands the records back with the bytes that carry their details.
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_commands::*, graphics_offscreen::*, graphics_properties::*,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, RenderTextureId, TargetRef, TextureRef, TextureSlot,
    snapshot_wire::decode_work,
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

/// Takes the frame's work and the decoded detail of each command record.
fn take(world: u64) -> Vec<(DisunityOffscreenWork, Option<OffscreenWork>)> {
    let mut count = 0;
    let mut records = [DisunityOffscreenWork::default(); 32];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 32, &mut count).code,
            0
        );
    }
    records[..count as usize]
        .iter()
        .map(|record| {
            let detail = (record.kind >= 4).then(|| {
                let mut length = 0;
                let mut bytes = [0_u8; 512];
                unsafe {
                    assert_eq!(
                        disunity_offscreen_detail(
                            world,
                            record.bindings,
                            std::ptr::null_mut(),
                            0,
                            &mut length
                        )
                        .code,
                        0
                    );
                    assert_eq!(
                        disunity_offscreen_detail(
                            world,
                            record.bindings,
                            bytes.as_mut_ptr(),
                            512,
                            &mut length
                        )
                        .code,
                        0
                    );
                }
                decode_work(&bytes[..length as usize]).unwrap()
            });
            (*record, detail)
        })
        .collect()
}

fn rejected(code: i32) {
    assert_ne!(code, 0);
}

/// Records one of every command and returns the color value and matrix it used.
fn record_every_command(world: u64, id: u64) -> ([f32; 4], [f32; 16]) {
    let colors = [target(0, 0, id), target(1, -77, 0), target(2, 0, 0)];
    let depth = target(1, 9, 0);
    let value = [0.25, 0.5, 0.75, 1.0];
    let matrix: [f32; 16] = std::array::from_fn(|i| i as f32);
    let slot = DisunityTextureSlot {
        kind: 2,
        texture: id,
        scale_x: 2.0,
        scale_y: 3.0,
        offset_x: 0.5,
        offset_y: 0.25,
    };
    let desc = description();
    let no_slot = std::ptr::null();
    let none = std::ptr::null();
    let (fname, flen) = text("_Float");
    let (iname, ilen) = text("_Int");
    let (vname, vlen) = text("_Vec");
    let (mname, mlen) = text("_Mat");
    let (tname, tlen) = text("_Tex");
    let (kname, klen) = text("_KEY");
    unsafe {
        let results = [
            disunity_offscreen_allocate_temporary(world, -77, &desc),
            disunity_offscreen_set_render_target(world, colors.as_ptr(), 3, &depth, 2, 4, 1),
            disunity_offscreen_clear_render_target(world, 1 | 4, value.as_ptr(), 0.5, 9),
            disunity_offscreen_set_global(world, 0, fname, flen, [1.5].as_ptr(), 0, no_slot),
            disunity_offscreen_set_global(world, 1, iname, ilen, none, -4, no_slot),
            disunity_offscreen_set_global(world, 2, vname, vlen, value.as_ptr(), 0, no_slot),
            disunity_offscreen_set_global(world, 3, mname, mlen, matrix.as_ptr(), 0, no_slot),
            disunity_offscreen_set_global(world, 4, tname, tlen, none, 0, &slot),
            disunity_offscreen_set_global_keyword(world, kname, klen, 1),
            disunity_offscreen_set_global_keyword(world, kname, klen, 0),
            disunity_offscreen_release_temporary(world, -77),
        ];
        for result in results {
            assert_eq!(result.code, 0);
        }
    }
    (value, matrix)
}

#[test]
fn every_command_call_is_recorded_in_order_and_taken_with_its_detail() {
    let world = new_world();
    let id = render_texture(world);
    let (value, matrix) = record_every_command(world, id);
    let taken = take(world);
    let kinds: Vec<u32> = taken.iter().map(|(record, _)| record.kind).collect();
    assert_eq!(kinds, [8, 4, 5, 6, 6, 6, 6, 6, 7, 7, 9]);
    let details: Vec<OffscreenWork> = taken.into_iter().map(|(_, d)| d.unwrap()).collect();
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
    assert_global_details(&details[3..8], render, value, matrix);
    assert_eq!(
        details[8],
        OffscreenWork::SetGlobalKeyword {
            name: "_KEY".into(),
            enabled: true
        }
    );
    assert_eq!(
        details[9],
        OffscreenWork::SetGlobalKeyword {
            name: "_KEY".into(),
            enabled: false
        }
    );
    assert_eq!(
        details[10],
        OffscreenWork::ReleaseTemporary { name_id: -77 }
    );
    assert!(take(world).is_empty());
}

fn assert_global_details(
    details: &[OffscreenWork],
    render: RenderTextureId,
    value: [f32; 4],
    matrix: [f32; 16],
) {
    let slot = TextureSlot {
        texture: TextureRef::Render(render),
        scale: [2.0, 3.0],
        offset: [0.5, 0.25],
    };
    let expected = [
        PropertyValue::Float("_Float".into(), 1.5),
        PropertyValue::Int("_Int".into(), -4),
        PropertyValue::Vector("_Vec".into(), value),
        PropertyValue::Matrix("_Mat".into(), matrix),
        PropertyValue::Texture("_Tex".into(), slot),
    ];
    for (detail, value) in details.iter().zip(expected) {
        assert_eq!(*detail, OffscreenWork::SetGlobal(value));
    }
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
    assert!(take(world).is_empty());
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
    assert!(take(world).is_empty());
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
    assert!(take(world).is_empty());
}

#[test]
fn a_global_call_with_a_bad_argument_records_nothing() {
    let world = new_world();
    let (name, length) = text("_G");
    let floats = [1.0_f32; 16];
    let nan = [f32::NAN];
    let dead_slot = DisunityTextureSlot {
        kind: 2,
        texture: 77,
        scale_x: 1.0,
        scale_y: 1.0,
        ..Default::default()
    };
    let none: *const f32 = std::ptr::null();
    let no_slot: *const DisunityTextureSlot = std::ptr::null();
    unsafe {
        rejected(disunity_offscreen_set_global(world, 0, name, length, none, 0, no_slot).code);
        rejected(
            disunity_offscreen_set_global(world, 0, name, length, nan.as_ptr(), 0, no_slot).code,
        );
        rejected(disunity_offscreen_set_global(world, 2, name, length, none, 0, no_slot).code);
        rejected(disunity_offscreen_set_global(world, 3, name, length, none, 0, no_slot).code);
        rejected(
            disunity_offscreen_set_global(world, 4, name, length, floats.as_ptr(), 0, no_slot).code,
        );
        rejected(disunity_offscreen_set_global(world, 4, name, length, none, 0, &dead_slot).code);
        rejected(
            disunity_offscreen_set_global(world, 5, name, length, floats.as_ptr(), 0, no_slot).code,
        );
        rejected(disunity_offscreen_set_global(world, 1, none.cast(), 0, none, 0, no_slot).code);
        rejected(
            disunity_offscreen_set_global(world, 1, std::ptr::null(), 2, none, 0, no_slot).code,
        );
    }
    assert!(take(world).is_empty());
}

#[test]
fn keyword_and_temporary_calls_with_bad_arguments_record_nothing() {
    let world = new_world();
    let (name, length) = text("_K");
    let wide = DisunityRenderTexture {
        width: 0,
        ..description()
    };
    let srgb = DisunityRenderTexture {
        srgb: 2,
        ..description()
    };
    unsafe {
        rejected(disunity_offscreen_set_global_keyword(world, name, length, 2).code);
        rejected(disunity_offscreen_set_global_keyword(world, name, 0, 1).code);
        rejected(disunity_offscreen_set_global_keyword(world, std::ptr::null(), 1, 1).code);
        rejected(disunity_offscreen_allocate_temporary(world, 1, std::ptr::null()).code);
        rejected(disunity_offscreen_allocate_temporary(world, 1, &wide).code);
        rejected(disunity_offscreen_allocate_temporary(world, 1, &srgb).code);
    }
    rejected(disunity_offscreen_release_temporary(world + 99, 1).code);
    assert!(take(world).is_empty());
}

#[test]
fn detail_reads_need_a_token_of_the_last_take_and_room() {
    let world = new_world();
    assert_eq!(disunity_offscreen_release_temporary(world, 3).code, 0);
    assert_eq!(disunity_offscreen_release_temporary(world, 4).code, 0);
    let mut count = 0;
    let mut records = [DisunityOffscreenWork::default(); 2];
    let mut length = 0;
    let mut small = [0_u8; 1];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 2, &mut count).code,
            0
        );
        assert_eq!((records[0].bindings, records[1].bindings), (1, 2));
        assert_eq!(
            disunity_offscreen_detail(world, 2, std::ptr::null_mut(), 0, &mut length).code,
            0
        );
        assert_eq!(length, 5);
        rejected(disunity_offscreen_detail(world, 2, small.as_mut_ptr(), 1, &mut length).code);
        rejected(disunity_offscreen_detail(world, 0, std::ptr::null_mut(), 0, &mut length).code);
        rejected(disunity_offscreen_detail(world, 3, std::ptr::null_mut(), 0, &mut length).code);
        rejected(
            disunity_offscreen_detail(world, 1, std::ptr::null_mut(), 0, std::ptr::null_mut()).code,
        );
        rejected(disunity_offscreen_detail(world, 1, std::ptr::null_mut(), 5, &mut length).code);
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 2, &mut count).code,
            0
        );
        rejected(disunity_offscreen_detail(world, 1, std::ptr::null_mut(), 0, &mut length).code);
    }
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
    assert!(take(world).is_empty());
}
