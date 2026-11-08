//! Reading pixels back from a render texture through the ABI: the script records a request, the
//! host writes the texels and the script takes them once.
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_commands::*, graphics_offscreen::*, graphics_readback::*,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, snapshot_wire::decode_work,
};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn render_texture(world: u64) -> u64 {
    let desc = DisunityRenderTexture {
        width: 8,
        height: 4,
        format: 8,
        mip_count: 1,
        filter: 1,
        aniso_level: 1,
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

fn request(world: u64, texture: u64, rect: [u32; 4]) -> Result<u64, i32> {
    let mut id = 0;
    let result = unsafe {
        disunity_render_texture_read_pixels(
            world, texture, rect[0], rect[1], rect[2], rect[3], &mut id,
        )
    };
    if result.code == 0 {
        Ok(id)
    } else {
        Err(result.code)
    }
}

fn write(world: u64, request: u64, data: &[f32]) -> i32 {
    unsafe {
        disunity_render_texture_pixels_write(
            world,
            request,
            data.as_ptr(),
            u32::try_from(data.len()).unwrap(),
        )
    }
    .code
}

/// The call result and the floats written, from a buffer of `capacity` floats.
fn take(world: u64, request: u64, capacity: usize) -> (i32, Vec<f32>) {
    let mut data = vec![-1.0_f32; capacity];
    let mut written = 77_u32;
    let code = unsafe {
        disunity_render_texture_pixels_take(
            world,
            request,
            data.as_mut_ptr(),
            u32::try_from(capacity).unwrap(),
            &mut written,
        )
    }
    .code;
    if code != 0 {
        return (code, Vec::new());
    }
    data.truncate(written as usize);
    (0, data)
}

fn floats(count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| f32::from(u16::try_from(i).unwrap()) + 0.25)
        .collect()
}

#[test]
fn a_request_returns_an_id_and_is_taken_with_its_detail() {
    let world = new_world();
    let texture = render_texture(world);
    let first = request(world, texture, [1, 2, 3, 1]).unwrap();
    let second = request(world, texture, [0, 0, 8, 4]).unwrap();
    assert!(first > 0 && second > first);

    let mut count = 0;
    let mut records = [DisunityOffscreenWork::default(); 4];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 4, &mut count).code,
            0
        );
    }
    assert_eq!(count, 2);
    assert_eq!(records[0].kind, KIND_READ_PIXELS);
    let mut length = 0;
    let mut bytes = [0_u8; 128];
    unsafe {
        assert_eq!(
            disunity_offscreen_detail(
                world,
                records[0].bindings,
                bytes.as_mut_ptr(),
                128,
                &mut length
            )
            .code,
            0
        );
    }
    assert_eq!(
        decode_work(&bytes[..length as usize]).unwrap(),
        OffscreenWork::ReadPixels {
            request: first,
            texture: RenderTextureId::from_bits(texture),
            x: 1,
            y: 2,
            width: 3,
            height: 1,
        }
    );
}

#[test]
fn a_rejected_request_leaves_the_id_alone_and_records_nothing() {
    let world = new_world();
    let texture = render_texture(world);
    for rect in [
        [0, 0, 0, 1],
        [0, 0, 1, 0],
        [0, 0, 9, 1],
        [7, 0, 2, 1],
        [0, 3, 1, 2],
    ] {
        assert!(request(world, texture, rect).is_err(), "{rect:?}");
    }
    assert!(request(world, texture + 1000, [0, 0, 1, 1]).is_err());
    let mut id = 55;
    let result =
        unsafe { disunity_render_texture_read_pixels(world, texture, 0, 0, 0, 1, &mut id) };
    assert_ne!(result.code, 0);
    assert_eq!(id, 55);
    let result = unsafe {
        disunity_render_texture_read_pixels(world, texture, 0, 0, 1, 1, std::ptr::null_mut())
    };
    assert_ne!(result.code, 0);
    let mut count = 0;
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, std::ptr::null_mut(), 0, &mut count).code,
            0
        );
    }
    assert_eq!(count, 0);
}

#[test]
fn the_host_answer_is_taken_once() {
    let world = new_world();
    let texture = render_texture(world);
    let id = request(world, texture, [0, 0, 2, 3]).unwrap();
    let pixels = floats(24);
    assert_eq!(write(world, id, &pixels), 0);
    assert_eq!(take(world, id, 24), (0, pixels));
    assert_eq!(take(world, id, 24), (0, Vec::new()));
}

#[test]
fn a_request_nobody_answered_takes_zero_floats() {
    let world = new_world();
    let texture = render_texture(world);
    let id = request(world, texture, [0, 0, 2, 2]).unwrap();
    assert_eq!(take(world, id, 16), (0, Vec::new()));
    assert_ne!(write(world, id, &floats(16)), 0);
}

#[test]
fn writes_are_checked_for_request_length_and_pointer() {
    let world = new_world();
    let texture = render_texture(world);
    let id = request(world, texture, [0, 0, 2, 2]).unwrap();
    assert_ne!(write(world, id + 1, &floats(16)), 0);
    assert_ne!(write(world, 0, &floats(16)), 0);
    assert_ne!(write(world, id, &floats(15)), 0);
    assert_ne!(write(world, id, &floats(17)), 0);
    assert_ne!(write(world, id, &[]), 0);
    let null = unsafe { disunity_render_texture_pixels_write(world, id, std::ptr::null(), 16) };
    assert_ne!(null.code, 0);
    assert_eq!(write(world, id, &floats(16)), 0);
}

#[test]
fn a_take_with_too_little_room_is_refused_and_keeps_the_answer() {
    let world = new_world();
    let texture = render_texture(world);
    let id = request(world, texture, [0, 0, 2, 2]).unwrap();
    assert_eq!(write(world, id, &floats(16)), 0);
    assert_ne!(take(world, id, 15).0, 0);
    assert_eq!(take(world, id, 16), (0, floats(16)));
}

#[test]
fn a_take_checks_its_pointers_and_the_request() {
    let world = new_world();
    let texture = render_texture(world);
    let id = request(world, texture, [0, 0, 1, 1]).unwrap();
    assert_eq!(write(world, id, &floats(4)), 0);
    let mut data = [0.0_f32; 4];
    let mut written = 0_u32;
    unsafe {
        let refused = [
            disunity_render_texture_pixels_take(
                world,
                id,
                data.as_mut_ptr(),
                4,
                std::ptr::null_mut(),
            ),
            disunity_render_texture_pixels_take(world, id, std::ptr::null_mut(), 4, &mut written),
        ];
        assert!(refused.iter().all(|result| result.code != 0));
    }
    assert_eq!(take(world, id, 4), (0, floats(4)));
}
