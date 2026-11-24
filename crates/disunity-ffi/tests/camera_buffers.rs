//! A camera's target buffers through the ABI: set and read back, replaced by a target texture and
//! carried by the camera setup record.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{
    abi::*, camera::*, graphics::*, graphics_commands::*, graphics_context::*, sprites::*,
};
use worldrunner_world_simulation::graphics::{CameraBuffers, OffscreenWork, RenderTextureId};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn camera(world: u64) -> u64 {
    let (mut object, mut camera) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_camera_create(world, object, &mut camera).code, 0);
    }
    camera
}

fn render_texture(world: u64) -> u64 {
    let desc = DisunityRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        depth_bits: 24,
        mip_count: 1,
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

fn set(world: u64, camera: u64, colors: &[u64], depth: u64) -> i32 {
    let list = if colors.is_empty() {
        std::ptr::null()
    } else {
        colors.as_ptr()
    };
    unsafe {
        disunity_camera_target_buffers_write(world, camera, list, colors.len() as u32, depth).code
    }
}

fn read(world: u64, camera: u64) -> (Vec<u64>, u64) {
    let (mut colors, mut count, mut depth) = ([0_u64; 8], 0_u32, 0_u64);
    unsafe {
        assert_eq!(
            disunity_camera_target_buffers_read(
                world,
                camera,
                colors.as_mut_ptr(),
                &mut count,
                &mut depth
            )
            .code,
            0
        );
    }
    (colors[..count as usize].to_vec(), depth)
}

fn target_texture(world: u64, camera: u64) -> u64 {
    let mut value = DisunityCamera::default();
    unsafe { assert_eq!(disunity_camera_read(world, camera, &mut value).code, 0) };
    value.target_texture
}

#[test]
fn a_camera_has_no_buffers_until_they_are_set_and_reads_them_back_in_order() {
    let world = new_world();
    let camera = camera(world);
    assert_eq!(read(world, camera), (vec![], 0));
    let ids: Vec<u64> = (0..4).map(|_| render_texture(world)).collect();
    assert_eq!(set(world, camera, &ids[..3], ids[3]), 0);
    assert_eq!(read(world, camera), (ids[..3].to_vec(), ids[3]));
    assert_eq!(set(world, camera, &ids[..1], 0), 0);
    assert_eq!(read(world, camera), (ids[..1].to_vec(), 0));
    assert_eq!(set(world, camera, &[], 0), 0);
    assert_eq!(read(world, camera), (vec![], 0));
}

#[test]
fn buffers_replace_the_target_texture_and_a_target_texture_replaces_them() {
    let world = new_world();
    let camera = camera(world);
    let (first, second, third) = (
        render_texture(world),
        render_texture(world),
        render_texture(world),
    );
    let mut value = DisunityCamera {
        target_texture: third,
        ..DisunityCamera::default()
    };
    unsafe { assert_eq!(disunity_camera_write(world, camera, &value).code, 0) };
    assert_eq!(set(world, camera, &[first, second], 0), 0);
    assert_eq!(
        target_texture(world, camera),
        0,
        "the buffers clear the target texture"
    );
    // A write of the other settings keeps the buffers, since it carries no target texture.
    value.target_texture = 0;
    value.depth = 4;
    unsafe { assert_eq!(disunity_camera_write(world, camera, &value).code, 0) };
    assert_eq!(read(world, camera).0, [first, second]);
    value.target_texture = third;
    unsafe { assert_eq!(disunity_camera_write(world, camera, &value).code, 0) };
    assert_eq!(
        read(world, camera),
        (vec![], 0),
        "a target texture clears the buffers"
    );
    assert_eq!(target_texture(world, camera), third);
    // Removing buffers that are not there leaves the target texture alone.
    assert_eq!(set(world, camera, &[], 0), 0);
    assert_eq!(target_texture(world, camera), third);
}

#[test]
fn buffers_the_world_cannot_draw_into_are_refused_and_change_nothing() {
    let world = new_world();
    let camera = camera(world);
    let ids: Vec<u64> = (0..10).map(|_| render_texture(world)).collect();
    assert_eq!(set(world, camera, &ids[..2], ids[2]), 0);
    let before = read(world, camera);
    let dead = render_texture(world);
    assert_eq!(disunity_render_texture_destroy(world, dead).code, 0);
    for (what, colors, depth) in [
        ("nine colors", &ids[..9], 0),
        ("one texture twice", &[ids[0], ids[0]][..], 0),
        ("a stale color", &[ids[0], dead][..], 0),
        ("a stale depth", &ids[..1], dead),
        ("a depth with no color", &[][..], ids[3]),
    ] {
        assert_ne!(set(world, camera, colors, depth), 0, "{what}");
        assert_eq!(read(world, camera), before, "{what} leaves the buffers");
    }
    unsafe {
        assert_ne!(
            disunity_camera_target_buffers_write(world, camera, std::ptr::null(), 2, 0).code,
            0
        );
        assert_ne!(
            disunity_camera_target_buffers_read(
                world,
                camera,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut()
            )
            .code,
            0
        );
    }
}

#[test]
fn a_camera_setup_carries_the_buffers_it_is_given_with_the_camera_target() {
    let world = new_world();
    let camera = camera(world);
    let ids: Vec<u64> = (0..4).map(|_| render_texture(world)).collect();
    let matrices: Vec<f32> = [1.0_f32; 16].into_iter().chain([2.0_f32; 16]).collect();
    let camera_target = DisunityTargetRef {
        kind: 2,
        name_id: 0,
        render: 0,
    };
    unsafe {
        assert_eq!(
            disunity_offscreen_setup_camera(
                world,
                camera,
                matrices.as_ptr(),
                &camera_target,
                ids.as_ptr(),
                3,
                ids[3]
            )
            .code,
            0
        );
        assert_eq!(
            disunity_offscreen_setup_camera(
                world,
                camera,
                matrices.as_ptr(),
                &camera_target,
                std::ptr::null(),
                0,
                0
            )
            .code,
            0
        );
        // Buffers go with the camera target alone, a depth needs a color, and nine are too many.
        let texture = DisunityTargetRef {
            kind: 0,
            name_id: 0,
            render: ids[0],
        };
        let nine = [ids[0]; 9];
        for (target, colors, count, depth) in [
            (&texture, ids.as_ptr(), 1, 0),
            (&camera_target, std::ptr::null(), 0, ids[0]),
            (&camera_target, nine.as_ptr(), 9, 0),
            (&camera_target, std::ptr::null(), 1, 0),
        ] {
            assert_ne!(
                disunity_offscreen_setup_camera(
                    world,
                    camera,
                    matrices.as_ptr(),
                    target,
                    colors,
                    count,
                    depth
                )
                .code,
                0
            );
        }
    }
    let decoded = snapshot(world).work;
    assert_eq!(decoded.len(), 2, "only the two accepted calls recorded");
    let want = CameraBuffers::new(
        &ids[..3]
            .iter()
            .map(|id| RenderTextureId::from_bits(*id))
            .collect::<Vec<_>>(),
        Some(RenderTextureId::from_bits(ids[3])),
    )
    .unwrap();
    let OffscreenWork::SetupCamera { buffers, .. } = &decoded[0] else {
        panic!("a camera setup");
    };
    assert_eq!(*buffers, Some(want));
    let OffscreenWork::SetupCamera { buffers, .. } = &decoded[1] else {
        panic!("a camera setup");
    };
    assert_eq!(*buffers, None);
}
