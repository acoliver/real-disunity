//! A render texture's two ways of getting its mips through the ABI: the `auto_generate_mips`
//! flag of its description, and the generate-mips call that records a script's request. The call
//! forwards one record, a take hands it back with kind 19 and the bytes that carry it, and the
//! graphics snapshot carries both the flag and the record to the host.
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_commands::*, graphics_context::*, graphics_offscreen::*,
    graphics_snapshot::*,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId,
    snapshot_wire::{self, decode_work},
};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn render_texture(world: u64, mip_count: u32, auto_generate_mips: u32) -> u64 {
    let desc = DisunityRenderTexture {
        width: 16,
        height: 16,
        format: 8,
        mip_count,
        auto_generate_mips,
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

fn pending(world: u64) -> u32 {
    let mut count = 0;
    unsafe {
        let result = disunity_offscreen_take(world, std::ptr::null_mut(), 0, &mut count);
        assert_eq!(result.code, 0);
    }
    count
}

#[test]
fn the_flag_round_trips_through_the_description() {
    let world = new_world();
    let id = render_texture(world, 5, 1);
    let mut read = DisunityRenderTexture::default();
    unsafe { assert_eq!(disunity_render_texture_read(world, id, &mut read).code, 0) };
    assert_eq!(read.auto_generate_mips, 1);
    read.auto_generate_mips = 0;
    unsafe { assert_eq!(disunity_render_texture_write(world, id, &read).code, 0) };
    let mut again = DisunityRenderTexture::default();
    unsafe { assert_eq!(disunity_render_texture_read(world, id, &mut again).code, 0) };
    assert_eq!(again.auto_generate_mips, 0);
    read.auto_generate_mips = 2;
    unsafe { assert_ne!(disunity_render_texture_write(world, id, &read).code, 0) };
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn the_call_reaches_the_frame_as_one_record_of_kind_nineteen() {
    let world = new_world();
    let id = render_texture(world, 4, 0);
    assert_eq!(disunity_offscreen_generate_mips(world, id).code, 0);
    let mut count = 0;
    let mut record = [DisunityOffscreenWork::default()];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, record.as_mut_ptr(), 1, &mut count).code,
            0
        );
    }
    assert_eq!(count, 1);
    assert_eq!(record[0].kind, KIND_GENERATE_MIPS);
    assert_eq!(KIND_GENERATE_MIPS, 19);
    let (mut length, mut bytes) = (0, [0_u8; 64]);
    unsafe {
        assert_eq!(
            disunity_offscreen_detail(
                world,
                record[0].bindings,
                bytes.as_mut_ptr(),
                64,
                &mut length
            )
            .code,
            0
        );
    }
    assert_eq!(
        decode_work(&bytes[..length as usize]).unwrap(),
        OffscreenWork::GenerateMips {
            texture: RenderTextureId::from_bits(id)
        }
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_texture_without_mips_or_a_dead_one_is_refused_and_records_nothing() {
    let world = new_world();
    let flat = render_texture(world, 1, 0);
    let dead = render_texture(world, 3, 0);
    assert_eq!(disunity_render_texture_destroy(world, dead).code, 0);
    assert_ne!(disunity_offscreen_generate_mips(world, flat).code, 0);
    assert_ne!(disunity_offscreen_generate_mips(world, dead).code, 0);
    assert_ne!(disunity_offscreen_generate_mips(world, 0).code, 0);
    assert_ne!(disunity_offscreen_generate_mips(world + 999, flat).code, 0);
    assert_eq!(pending(world), 0);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn the_snapshot_carries_the_flag_and_the_record_to_the_host() {
    let world = new_world();
    let flagged = render_texture(world, 5, 1);
    let manual = render_texture(world, 3, 0);
    assert_eq!(disunity_offscreen_generate_mips(world, manual).code, 0);
    let mut length = 0;
    unsafe {
        assert_eq!(
            disunity_graphics_snapshot(world, std::ptr::null_mut(), 0, &mut length).code,
            0
        );
    }
    let mut bytes = vec![0_u8; length as usize];
    unsafe {
        assert_eq!(
            disunity_graphics_snapshot(world, bytes.as_mut_ptr(), length, &mut length).code,
            0
        );
    }
    let snapshot = snapshot_wire::decode(&bytes).unwrap();
    let (flagged, manual) = (
        RenderTextureId::from_bits(flagged),
        RenderTextureId::from_bits(manual),
    );
    assert!(snapshot.render_textures[&flagged].auto_generate_mips);
    assert!(!snapshot.render_textures[&manual].auto_generate_mips);
    assert_eq!(
        snapshot.work,
        vec![OffscreenWork::GenerateMips { texture: manual }]
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
