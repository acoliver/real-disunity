//! Renderer lists through the ABI: pointer checks, the decoding of the pass tags, and the round
//! trip of the records through the taken work list.
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_commands::*, graphics_context::*, graphics_offscreen::*,
    graphics_renderer_lists::*, sprites::*,
};
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, RendererListDesc, snapshot_wire::decode_work,
};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn cull(world: u64, id: u64) {
    let (mut object, mut camera) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_camera_create(world, object, &mut camera).code, 0);
        assert_eq!(
            disunity_offscreen_cull(world, id, camera, u32::MAX, std::ptr::null(), 0, 0).code,
            0
        );
    }
}

fn material(world: u64) -> u64 {
    let (mut shader, mut material) = (0, 0);
    unsafe {
        assert_eq!(
            disunity_shader_create(world, b"Lit".as_ptr(), 3, &mut shader).code,
            0
        );
        assert_eq!(
            disunity_material_create(world, shader, &mut material).code,
            0
        );
    }
    material
}

fn draw_desc(tags: &[u8]) -> DisunityDrawRendererList {
    DisunityDrawRendererList {
        cull: 1,
        override_material: 0,
        queue_min: 1000,
        queue_max: 2500,
        layer_mask: 0xFFFF_FFFB,
        rendering_layer_mask: 0xFF,
        sorting: 0x1234,
        per_object: 0x56,
        override_pass: -1,
        tags_len: u32::try_from(tags.len()).unwrap(),
    }
}

fn create_draw(world: u64, desc: &DisunityDrawRendererList, tags: &[u8]) -> Result<u64, i32> {
    let mut list = 0;
    let pointer = if tags.is_empty() {
        std::ptr::null()
    } else {
        tags.as_ptr()
    };
    let result = unsafe { disunity_renderer_list_create_draw(world, desc, pointer, &mut list) };
    if result.code == 0 {
        Ok(list)
    } else {
        assert_eq!(list, 0, "a rejected call writes no id");
        Err(result.code)
    }
}

fn shadow_desc(plane_count: u32) -> DisunityShadowRendererList {
    DisunityShadowRendererList {
        sphere_x: 1.0,
        sphere_y: 2.0,
        sphere_z: 3.0,
        sphere_radius: 40.0,
        light_index: 2,
        plane_count,
        objects_filter: 1,
        projection: 2,
        rendering_layer_test: 1,
        padding: 0,
        cull: 1,
    }
}

fn create_shadow(
    world: u64,
    desc: &DisunityShadowRendererList,
    planes: &[f32],
) -> Result<u64, i32> {
    let mut list = 0;
    let pointer = if planes.is_empty() {
        std::ptr::null()
    } else {
        planes.as_ptr()
    };
    let result = unsafe { disunity_renderer_list_create_shadow(world, desc, pointer, &mut list) };
    if result.code == 0 {
        Ok(list)
    } else {
        assert_eq!(list, 0, "a rejected call writes no id");
        Err(result.code)
    }
}

/// Takes the frame's work and the decoded detail of each record.
fn take(world: u64) -> Vec<(DisunityOffscreenWork, OffscreenWork)> {
    let mut count = 0;
    let mut records = [DisunityOffscreenWork::default(); 16];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 16, &mut count).code,
            0
        );
    }
    records[..count as usize]
        .iter()
        .map(|record| {
            let mut length = 0;
            let mut bytes = [0_u8; 1024];
            unsafe {
                assert_eq!(
                    disunity_offscreen_detail(
                        world,
                        record.bindings,
                        bytes.as_mut_ptr(),
                        1024,
                        &mut length
                    )
                    .code,
                    0
                );
            }
            (*record, decode_work(&bytes[..length as usize]).unwrap())
        })
        .collect()
}

#[test]
fn the_kinds_follow_the_pixel_read_kind() {
    assert_eq!(KIND_CREATE_RENDERER_LIST, 13);
    assert_eq!(KIND_DRAW_RENDERER_LIST, 14);
}

#[test]
fn a_draw_list_round_trips_with_its_tags_and_fields() {
    let world = new_world();
    cull(world, 1);
    let override_material = material(world);
    let mut desc = draw_desc(b"UniversalForward\0SRPDefaultUnlit\0caf\xC3\xA9");
    desc.override_material = override_material;
    desc.override_pass = 2;
    let list = create_draw(
        world,
        &desc,
        b"UniversalForward\0SRPDefaultUnlit\0caf\xC3\xA9",
    )
    .unwrap();
    assert_ne!(list, 0);
    let taken = take(world);
    assert_eq!(taken.len(), 2);
    assert_eq!(taken[1].0.kind, KIND_CREATE_RENDERER_LIST);
    assert_ne!(taken[1].0.bindings, 0);
    assert_eq!(
        taken[1].1,
        OffscreenWork::CreateRendererList {
            id: list,
            desc: RendererListDesc::Draw {
                cull: 1,
                pass_tags: vec![
                    "UniversalForward".into(),
                    "SRPDefaultUnlit".into(),
                    "café".into()
                ],
                queue_min: 1000,
                queue_max: 2500,
                layer_mask: 0xFFFF_FFFB,
                rendering_layer_mask: 0xFF,
                sorting: 0x1234,
                per_object: 0x56,
                override_material: Some(MaterialId::from_bits(override_material)),
                override_pass: 2,
            },
        }
    );
}

#[test]
fn a_shadow_list_round_trips_with_its_planes_and_sphere() {
    let world = new_world();
    cull(world, 1);
    let planes: [f32; 8] = std::array::from_fn(|i| 0.5 * i as f32 - 1.5);
    let list = create_shadow(world, &shadow_desc(2), &planes).unwrap();
    let untested = DisunityShadowRendererList {
        rendering_layer_test: 0,
        ..shadow_desc(0)
    };
    let none = create_shadow(world, &untested, &[]).unwrap();
    assert_ne!(list, none);
    let not_a_flag = DisunityShadowRendererList {
        rendering_layer_test: 2,
        ..shadow_desc(0)
    };
    assert!(
        create_shadow(world, &not_a_flag, &[]).is_err(),
        "the layer test is 0 or 1"
    );
    let taken = take(world);
    assert_eq!(taken[1].0.kind, KIND_CREATE_RENDERER_LIST);
    assert_eq!(
        taken[1].1,
        OffscreenWork::CreateRendererList {
            id: list,
            desc: RendererListDesc::Shadow {
                cull: 1,
                light_index: 2,
                planes: vec![[-1.5, -1.0, -0.5, 0.0], [0.5, 1.0, 1.5, 2.0]],
                sphere: [1.0, 2.0, 3.0, 40.0],
                objects_filter: 1,
                projection: 2,
                use_rendering_layer_mask_test: true,
            },
        }
    );
    let OffscreenWork::CreateRendererList {
        desc:
            RendererListDesc::Shadow {
                planes,
                use_rendering_layer_mask_test,
                ..
            },
        ..
    } = &taken[2].1
    else {
        panic!("expected a shadow list, got {:?}", taken[2].1);
    };
    assert!(planes.is_empty() && !use_rendering_layer_mask_test);
    assert_eq!(taken.len(), 3, "the refused list recorded nothing");
}

#[test]
fn drawing_a_list_is_recorded_after_it_and_refuses_unknown_lists() {
    let world = new_world();
    cull(world, 1);
    let list = create_draw(world, &draw_desc(b"A"), b"A").unwrap();
    assert_ne!(
        disunity_offscreen_draw_renderer_list(world, list + 1).code,
        0
    );
    assert_ne!(disunity_offscreen_draw_renderer_list(world, 0).code, 0);
    assert_eq!(disunity_offscreen_draw_renderer_list(world, list).code, 0);
    let taken = take(world);
    assert_eq!(taken.len(), 3);
    assert_eq!(taken[2].0.kind, KIND_DRAW_RENDERER_LIST);
    assert_eq!(taken[2].1, OffscreenWork::DrawRendererList { list });
    assert_ne!(disunity_offscreen_draw_renderer_list(world, list).code, 0);
}

#[test]
fn null_pointers_and_oversized_inputs_are_refused_without_recording() {
    let world = new_world();
    cull(world, 1);
    let desc = draw_desc(b"A");
    let shadow = shadow_desc(1);
    let mut list = 0_u64;
    unsafe {
        let refused = [
            disunity_renderer_list_create_draw(world, std::ptr::null(), b"A".as_ptr(), &mut list),
            disunity_renderer_list_create_draw(world, &desc, std::ptr::null(), &mut list),
            disunity_renderer_list_create_draw(world, &desc, b"A".as_ptr(), std::ptr::null_mut()),
            disunity_renderer_list_create_shadow(
                world,
                std::ptr::null(),
                [0.0_f32; 4].as_ptr(),
                &mut list,
            ),
            disunity_renderer_list_create_shadow(world, &shadow, std::ptr::null(), &mut list),
            disunity_renderer_list_create_shadow(
                world,
                &shadow,
                [0.0_f32; 4].as_ptr(),
                std::ptr::null_mut(),
            ),
        ];
        assert!(refused.iter().all(|result| result.code != 0));
    }
    assert_eq!(list, 0);
    let too_long = vec![b'a'; 16 * 257 + 1];
    assert!(create_draw(world, &draw_desc(&too_long), &too_long).is_err());
    let planes = [0.0_f32; 44];
    assert!(create_shadow(world, &shadow_desc(11), &planes).is_err());
    // Ten planes are the most a list holds.
    assert!(create_shadow(world, &shadow_desc(10), &planes[..40]).is_ok());
    assert_eq!(take(world).len(), 2);
}

#[test]
fn the_worlds_validation_rejects_bad_tags_and_arguments() {
    let world = new_world();
    cull(world, 1);
    let bad_tags: [&[u8]; 5] = [
        b"",
        b"A\0\0B",
        b"A\0",
        b"\xFF\xFE",
        b"a\0b\0c\0d\0e\0f\0g\0h\0i\0j\0k\0l\0m\0n\0o\0p\0q",
    ];
    for tags in bad_tags {
        assert!(
            create_draw(world, &draw_desc(tags), tags).is_err(),
            "{tags:?}"
        );
    }
    let mut desc = draw_desc(b"A");
    desc.queue_min = 3000;
    assert!(create_draw(world, &desc, b"A").is_err());
    desc = draw_desc(b"A");
    desc.cull = 99;
    assert!(create_draw(world, &desc, b"A").is_err());
    desc = draw_desc(b"A");
    desc.override_material = 12345;
    assert!(create_draw(world, &desc, b"A").is_err());
    let mut shadow = shadow_desc(0);
    shadow.light_index = -1;
    assert!(create_shadow(world, &shadow, &[]).is_err());
    shadow = shadow_desc(1);
    assert!(create_shadow(world, &shadow, &[f32::NAN, 0.0, 0.0, 0.0]).is_err());
    shadow = shadow_desc(0);
    shadow.sphere_radius = f32::INFINITY;
    assert!(create_shadow(world, &shadow, &[]).is_err());
    assert_eq!(take(world).len(), 1);
}

#[test]
fn sixteen_tags_are_accepted() {
    let world = new_world();
    cull(world, 1);
    let tags = b"a\0b\0c\0d\0e\0f\0g\0h\0i\0j\0k\0l\0m\0n\0o\0p";
    create_draw(world, &draw_desc(tags), tags).unwrap();
}
