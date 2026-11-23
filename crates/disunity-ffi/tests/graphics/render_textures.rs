//! Render textures, the cameras that draw into them, offscreen work and shader reads through the ABI:
//! validation, and that accepted calls reach the world.
use super::frame::snapshot;
use super::*;
use worldrunner_world_simulation::graphics::{OffscreenWork, RenderTextureId, TargetRef};

#[test]
fn a_render_texture_round_trips_and_a_write_raises_its_generation() {
    let world = new_world();
    let id = render_texture(world, 3);
    let mut desc = DisunityRenderTexture::default();
    unsafe { assert_eq!(disunity_render_texture_read(world, id, &mut desc).code, 0) };
    assert_eq!((desc.width, desc.height, desc.format), (64, 32, 8));
    assert_eq!(
        (desc.depth_bits, desc.mip_count, desc.srgb, desc.generation),
        (24, 3, 1, 1)
    );
    desc.width = 128;
    unsafe { assert_eq!(disunity_render_texture_write(world, id, &desc).code, 0) };
    unsafe { assert_eq!(disunity_render_texture_read(world, id, &mut desc).code, 0) };
    assert_eq!((desc.width, desc.generation), (128, 2));
}

#[test]
fn a_render_texture_shape_round_trips() {
    let world = new_world();
    let desc = DisunityRenderTexture {
        width: 8,
        height: 8,
        format: 48,
        mip_count: 1,
        filter: 1,
        aniso_level: 1,
        dimension: 6,
        volume_depth: 12,
        ..Default::default()
    };
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_render_texture_create(world, &desc, &mut id).code,
            0
        )
    };
    let mut read = DisunityRenderTexture::default();
    unsafe { assert_eq!(disunity_render_texture_read(world, id, &mut read).code, 0) };
    assert_eq!((read.dimension, read.volume_depth), (6, 12));
}

#[test]
fn a_render_texture_sampler_round_trips_and_a_change_raises_the_generation() {
    let world = new_world();
    let id = render_texture(world, 1);
    let mut desc = DisunityRenderTexture::default();
    unsafe { assert_eq!(disunity_render_texture_read(world, id, &mut desc).code, 0) };
    assert_eq!((desc.wrap_u, desc.wrap_v, desc.wrap_w), (0, 0, 0));
    assert_eq!((desc.filter, desc.aniso_level, desc.generation), (1, 1, 1));
    desc.wrap_u = 1;
    desc.wrap_v = 2;
    desc.wrap_w = 3;
    desc.filter = 2;
    desc.aniso_level = 16;
    desc.mip_bias = -1.5;
    unsafe { assert_eq!(disunity_render_texture_write(world, id, &desc).code, 0) };
    let mut read = DisunityRenderTexture::default();
    unsafe { assert_eq!(disunity_render_texture_read(world, id, &mut read).code, 0) };
    assert_eq!((read.wrap_u, read.wrap_v, read.wrap_w), (1, 2, 3));
    assert_eq!((read.filter, read.aniso_level, read.generation), (2, 16, 2));
    assert_eq!(read.mip_bias.to_bits(), (-1.5f32).to_bits());
}

#[test]
fn a_render_texture_description_is_validated() {
    let world = new_world();
    let mut id = 0;
    let base = DisunityRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        depth_bits: 0,
        mip_count: 1,
        srgb: 0,
        filter: 1,
        aniso_level: 1,
        generation: 0,
        dimension: 2,
        volume_depth: 1,
        ..Default::default()
    };
    let bad = [
        DisunityRenderTexture { width: 0, ..base },
        DisunityRenderTexture {
            depth_bits: 8,
            ..base
        },
        DisunityRenderTexture { srgb: 2, ..base },
        DisunityRenderTexture {
            mip_count: 99,
            ..base
        },
        DisunityRenderTexture { wrap_u: 4, ..base },
        DisunityRenderTexture { wrap_v: 4, ..base },
        DisunityRenderTexture { wrap_w: 4, ..base },
        DisunityRenderTexture { filter: 3, ..base },
        DisunityRenderTexture {
            aniso_level: 17,
            ..base
        },
        DisunityRenderTexture {
            mip_bias: f32::NAN,
            ..base
        },
        DisunityRenderTexture {
            dimension: 0,
            ..base
        },
        DisunityRenderTexture {
            volume_depth: 2,
            ..base
        },
        DisunityRenderTexture {
            dimension: 4,
            width: 16,
            ..base
        },
    ];
    for desc in bad {
        let result = unsafe { disunity_render_texture_create(world, &desc, &mut id) };
        assert_ne!(result.code, 0, "{desc:?}");
    }
    let null = unsafe { disunity_render_texture_create(world, std::ptr::null(), &mut id) };
    assert_ne!(null.code, 0);
}

#[test]
fn a_camera_can_name_a_render_texture_and_reads_it_back() {
    let world = new_world();
    let target = render_texture(world, 1);
    let (mut object, mut camera) = (0, 0);
    let mut value = DisunityCamera::default();
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_camera_create(world, object, &mut camera).code, 0);
        value.target_texture = target;
        assert_eq!(disunity_camera_write(world, camera, &value).code, 0);
        let mut read = DisunityCamera::default();
        assert_eq!(disunity_camera_read(world, camera, &mut read).code, 0);
        assert_eq!(read.target_texture, target);
    }
}

fn camera_component(world: u64) -> u64 {
    let (mut object, mut camera) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_camera_create(world, object, &mut camera).code, 0);
    }
    camera
}

fn work(kind: u32) -> DisunityOffscreenWork {
    DisunityOffscreenWork {
        kind,
        ..DisunityOffscreenWork::default()
    }
}

#[test]
fn offscreen_work_is_recorded_in_order_and_taken_once() {
    let world = new_world();
    let camera = camera_component(world);
    let target = render_texture(world, 3);
    let material = material(world);
    let records = [
        DisunityOffscreenWork {
            camera,
            target,
            ..work(0)
        },
        DisunityOffscreenWork {
            source_kind: 2,
            source: target,
            target,
            material,
            pass: 1,
            ..work(1)
        },
        DisunityOffscreenWork {
            source_kind: 1,
            source: PLAYER_TEXTURE,
            source_element: 0,
            source_mip: 2,
            target,
            target_element: 0,
            target_mip: 1,
            ..work(2)
        },
    ];
    for record in &records {
        unsafe { assert_eq!(disunity_offscreen_record(world, record).code, 0) };
    }
    let frame = snapshot(world);
    let target_id = RenderTextureId::from_bits(target);
    let [
        OffscreenWork::CameraRender {
            camera: drawn_by,
            target: drawn_into,
        },
        blit,
        copy,
    ] = frame.work.as_slice()
    else {
        panic!(
            "expected a camera render, a blit and a copy, got {:?}",
            frame.work
        );
    };
    assert!(frame.cameras.contains_key(drawn_by), "the camera is listed");
    assert_eq!(*drawn_into, target_id);
    assert_eq!(
        *blit,
        OffscreenWork::Blit {
            source: TextureRef::Render(target_id),
            target: TargetRef::Render(target_id),
            material: Some(MaterialId::from_bits(material)),
            pass: 1,
        }
    );
    assert_eq!(
        *copy,
        OffscreenWork::CopyTexture {
            source: TextureRef::Asset(PLAYER_TEXTURE),
            source_element: 0,
            source_mip: 2,
            target: TargetRef::Render(target_id),
            target_element: 0,
            target_mip: 1,
        }
    );
    assert!(snapshot(world).work.is_empty(), "the work is taken once");
}

#[test]
fn offscreen_work_is_validated_before_it_is_recorded() {
    let world = new_world();
    let camera = camera_component(world);
    let target = render_texture(world, 1);
    let bad = [
        work(3),
        DisunityOffscreenWork {
            camera: camera + 99,
            target,
            ..work(0)
        },
        DisunityOffscreenWork {
            camera,
            target: target + (1 << 32),
            ..work(0)
        },
        DisunityOffscreenWork {
            target,
            material: 12345,
            ..work(1)
        },
        DisunityOffscreenWork {
            target,
            source_kind: 9,
            ..work(1)
        },
        DisunityOffscreenWork {
            target,
            pass: -5,
            ..work(1)
        },
        DisunityOffscreenWork {
            target,
            source_mip: 4,
            source_kind: 2,
            source: target,
            ..work(2)
        },
    ];
    for record in bad {
        let result = unsafe { disunity_offscreen_record(world, &record) };
        assert_ne!(result.code, 0, "{record:?}");
    }
    let null = unsafe { disunity_offscreen_record(world, std::ptr::null()) };
    assert_ne!(null.code, 0);
    assert!(snapshot(world).work.is_empty());
}

#[test]
fn a_shader_reads_back_its_name_and_source() {
    let world = new_world();
    let id = shader(world, "Terrain/Splat");
    let mut info = DisunityShaderInfo::default();
    unsafe {
        assert_eq!(
            disunity_shader_read(world, id, &mut info, std::ptr::null_mut()).code,
            0
        );
    }
    assert_eq!(
        (info.name_length, info.file_length, info.has_source),
        (13, 0, 0)
    );
    let (file, length) = text("sharedassets3.assets");
    unsafe {
        assert_eq!(
            disunity_shader_source_write(world, id, file, length, -9).code,
            0
        );
        assert_eq!(
            disunity_shader_read(world, id, &mut info, std::ptr::null_mut()).code,
            0
        );
    }
    assert_eq!(
        (
            info.name_length,
            info.file_length,
            info.has_source,
            info.path_id
        ),
        (13, 20, 1, -9)
    );
    assert_eq!(info.text_capacity, 33);
    let mut bytes = [0_u8; 33];
    unsafe {
        assert_eq!(
            disunity_shader_read(world, id, &mut info, bytes.as_mut_ptr()).code,
            0
        );
    }
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        "Terrain/Splatsharedassets3.assets"
    );
}

#[test]
fn a_shader_read_needs_room_for_its_text_and_a_live_shader() {
    let world = new_world();
    let id = shader(world, "Terrain/Splat");
    let (file, length) = text("sharedassets3.assets");
    let mut info = DisunityShaderInfo::default();
    unsafe {
        assert_eq!(
            disunity_shader_source_write(world, id, file, length, -9).code,
            0
        );
        assert_eq!(
            disunity_shader_read(world, id, &mut info, std::ptr::null_mut()).code,
            0
        );
    }
    info.text_capacity = 32;
    let mut short = [0_u8; 32];
    unsafe {
        assert_ne!(
            disunity_shader_read(world, id, &mut info, short.as_mut_ptr()).code,
            0
        );
        assert_ne!(
            disunity_shader_read(world, id + 1, &mut info, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_shader_read(world, id, std::ptr::null_mut(), std::ptr::null_mut()).code,
            0
        );
    }
    assert_eq!(short, [0; 32]);
}
