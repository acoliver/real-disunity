//! Shaders, materials, shader properties, render textures and offscreen work through the ABI:
//! validation, and that accepted calls reach the world.
use worldrunner_disunity_ffi::{
    abi::*, camera::*, graphics::*, graphics_offscreen::*, graphics_properties::*,
    mesh_renderer::*, sprites::*, with_native_world,
};
use worldrunner_world_simulation::graphics::{MaterialId, ShaderId};

const SCOPE_MATERIAL: u32 = 0;
const SCOPE_GLOBAL: u32 = 1;
const SCOPE_BLOCK: u32 = 2;
const PLAYER_TEXTURE: u64 = (1 << 40) + 3;

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn text(name: &str) -> (*const u8, u32) {
    (name.as_ptr(), u32::try_from(name.len()).unwrap())
}

fn shader(world: u64, name: &str) -> u64 {
    let (bytes, length) = text(name);
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_shader_create(world, bytes, length, &mut id).code,
            0
        )
    };
    id
}

fn material(world: u64) -> u64 {
    let mut id = 0;
    let shader = shader(world, "Hidden/Splat");
    unsafe { assert_eq!(disunity_material_create(world, shader, &mut id).code, 0) };
    id
}

fn render_texture(world: u64, mip_count: u32) -> u64 {
    let desc = DisunityRenderTexture {
        width: 64,
        height: 32,
        format: 8,
        depth_bits: 24,
        mip_count,
        srgb: 1,
        filter: 1,
        aniso_level: 1,
        generation: 0,
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

fn renderer(world: u64) -> u64 {
    let (mut object, mut renderer) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_mesh_renderer_create(world, object, 0, &mut renderer).code,
            0
        );
    }
    renderer
}

fn read_property(world: u64, scope: u32, target: u64, name: &str) -> DisunityPropertyRead {
    let (bytes, length) = text(name);
    let mut value = DisunityPropertyRead::default();
    let result =
        unsafe { disunity_shader_property_read(world, scope, target, bytes, length, &mut value) };
    assert_eq!(result.code, 0);
    value
}

fn set_float(world: u64, scope: u32, target: u64, name: &str, value: f32) -> DisunityResult {
    let (bytes, length) = text(name);
    unsafe { disunity_shader_float_write(world, scope, target, bytes, length, value) }
}

#[test]
fn a_shader_is_created_with_a_name_and_gains_a_source() {
    let world = new_world();
    let id = shader(world, "Terrain/Splat");
    let (file, length) = text("sharedassets3.assets");
    unsafe {
        assert_eq!(
            disunity_shader_source_write(world, id, file, length, -9).code,
            0
        );
    }
    with_native_world(world, |native| {
        let held = native.shader(ShaderId::from_bits(id)).unwrap();
        assert_eq!(held.name, "Terrain/Splat");
        let source = held.source.as_ref().unwrap();
        assert_eq!(
            (source.file.as_str(), source.path_id),
            ("sharedassets3.assets", -9)
        );
    })
    .unwrap();
    unsafe {
        assert_eq!(
            disunity_shader_source_write(world, id, std::ptr::null(), 0, 0).code,
            0
        );
    }
    with_native_world(world, |native| {
        assert!(
            native
                .shader(ShaderId::from_bits(id))
                .unwrap()
                .source
                .is_none()
        );
    })
    .unwrap();
}

#[test]
fn strings_must_be_utf8_non_null_and_non_empty() {
    let world = new_world();
    let mut id = 0;
    let invalid = [0xFF_u8, 0xFE];
    unsafe {
        let null = disunity_shader_create(world, std::ptr::null(), 3, &mut id);
        assert_ne!(null.code, 0);
        let empty = disunity_shader_create(world, b"x".as_ptr(), 0, &mut id);
        assert_ne!(empty.code, 0);
        let bad = disunity_shader_create(world, invalid.as_ptr(), 2, &mut id);
        assert_ne!(bad.code, 0);
        let no_output = disunity_shader_create(world, b"x".as_ptr(), 1, std::ptr::null_mut());
        assert_ne!(no_output.code, 0);
    }
    with_native_world(world, |native| assert_eq!(native.shaders().count(), 0)).unwrap();
}

#[test]
fn a_material_follows_its_shader_queue_and_passes() {
    let world = new_world();
    let first = shader(world, "A");
    let second = shader(world, "B");
    let mut id = 0;
    unsafe {
        assert_eq!(disunity_material_create(world, first, &mut id).code, 0);
        assert_eq!(disunity_material_shader_write(world, id, second).code, 0);
        assert_eq!(
            disunity_material_render_queue_write(world, id, 3000).code,
            0
        );
        let (name, length) = text("ShadowCaster");
        assert_eq!(
            disunity_material_pass_write(world, id, name, length, 0).code,
            0
        );
    }
    let mut info = DisunityMaterialInfo::default();
    unsafe { assert_eq!(disunity_material_read(world, id, &mut info).code, 0) };
    assert_eq!((info.shader, info.render_queue), (second, 3000));
    assert_eq!((info.keyword_count, info.disabled_pass_count), (0, 1));
    assert_eq!(info.generation, 4);
}

#[test]
fn a_material_calls_reject_stale_ids() {
    let world = new_world();
    let id = material(world);
    unsafe {
        assert_eq!(disunity_material_destroy(world, id).code, 0);
        assert_ne!(disunity_material_destroy(world, id).code, 0);
        assert_ne!(disunity_material_render_queue_write(world, id, 1).code, 0);
        let mut info = DisunityMaterialInfo::default();
        assert_ne!(disunity_material_read(world, id, &mut info).code, 0);
        let mut made = 0;
        assert_ne!(disunity_material_create(world, 0, &mut made).code, 0);
    }
}

#[test]
fn property_values_reach_the_material_the_globals_and_a_renderers_block() {
    let world = new_world();
    let material = material(world);
    let renderer = renderer(world);
    for (scope, target) in [
        (SCOPE_MATERIAL, material),
        (SCOPE_GLOBAL, 0),
        (SCOPE_BLOCK, renderer),
    ] {
        assert_eq!(set_float(world, scope, target, "_Cutoff", 0.25).code, 0);
        let value = read_property(world, scope, target, "_Cutoff");
        assert_eq!(
            (value.present, value.float_value.to_bits()),
            (1, 0.25_f32.to_bits())
        );
        let missing = read_property(world, scope, target, "_Missing");
        assert_eq!(missing.present, 0);
    }
}

#[test]
fn every_property_kind_round_trips_through_a_material() {
    let world = new_world();
    let id = material(world);
    let (int, vector, matrix) = (text("_Mode"), text("_Tint"), text("_Rot"));
    let values = [0.5_f32, 1.5, 2.5, 3.5];
    let mut matrix_values = [0.0_f32; 16];
    matrix_values[5] = 4.0;
    unsafe {
        assert_eq!(
            disunity_shader_int_write(world, SCOPE_MATERIAL, id, int.0, int.1, -3).code,
            0
        );
        assert_eq!(
            disunity_shader_vector_write(
                world,
                SCOPE_MATERIAL,
                id,
                vector.0,
                vector.1,
                values.as_ptr()
            )
            .code,
            0
        );
        assert_eq!(
            disunity_shader_matrix_write(
                world,
                SCOPE_MATERIAL,
                id,
                matrix.0,
                matrix.1,
                matrix_values.as_ptr()
            )
            .code,
            0
        );
    }
    let read = read_property(world, SCOPE_MATERIAL, id, "_Mode");
    assert_eq!((read.present, read.int_value), (2, -3));
    let read = read_property(world, SCOPE_MATERIAL, id, "_Tint");
    assert_eq!(read.present, 4);
    assert_eq!(
        [read.vector_x, read.vector_y, read.vector_z, read.vector_w].map(f32::to_bits),
        values.map(f32::to_bits)
    );
    with_native_world(world, |native| {
        let held = native.material(MaterialId::from_bits(id)).unwrap();
        assert_eq!(held.properties.matrix("_Rot"), Some(matrix_values));
    })
    .unwrap();
}

#[test]
fn a_texture_slot_carries_asset_render_and_none_with_scale_and_offset() {
    let world = new_world();
    let id = material(world);
    let target = render_texture(world, 1);
    let (name, length) = text("_Splat");
    let slot = |kind, texture| DisunityTextureSlot {
        kind,
        texture,
        scale_x: 2.0,
        scale_y: 3.0,
        offset_x: 0.5,
        offset_y: 0.25,
    };
    for (kind, texture) in [(1, PLAYER_TEXTURE), (2, target), (0, 0)] {
        let value = slot(kind, texture);
        unsafe {
            let written =
                disunity_shader_texture_write(world, SCOPE_MATERIAL, id, name, length, &value);
            assert_eq!(written.code, 0);
        }
        let read = read_property(world, SCOPE_MATERIAL, id, "_Splat");
        assert_eq!(read.present, 16);
        assert_eq!((read.texture_kind, read.texture), (kind, texture));
        assert_eq!(
            [read.scale_x, read.scale_y, read.offset_x, read.offset_y].map(f32::to_bits),
            [2.0_f32, 3.0, 0.5, 0.25].map(f32::to_bits)
        );
    }
}

#[test]
fn texture_slots_reject_unknown_kinds_low_identities_and_stale_render_textures() {
    let world = new_world();
    let id = material(world);
    let target = render_texture(world, 1);
    assert_eq!(disunity_render_texture_destroy(world, target).code, 0);
    let (name, length) = text("_Splat");
    let bad = [
        DisunityTextureSlot {
            kind: 3,
            ..DisunityTextureSlot::default()
        },
        DisunityTextureSlot {
            kind: 1,
            texture: 5,
            ..DisunityTextureSlot::default()
        },
        DisunityTextureSlot {
            kind: 1,
            texture: 0,
            ..DisunityTextureSlot::default()
        },
        DisunityTextureSlot {
            kind: 2,
            texture: target,
            ..DisunityTextureSlot::default()
        },
        DisunityTextureSlot {
            kind: 0,
            texture: 9,
            ..DisunityTextureSlot::default()
        },
        DisunityTextureSlot {
            kind: 0,
            scale_x: f32::NAN,
            ..DisunityTextureSlot::default()
        },
    ];
    for value in bad {
        let result = unsafe {
            disunity_shader_texture_write(world, SCOPE_MATERIAL, id, name, length, &value)
        };
        assert_ne!(result.code, 0, "{value:?}");
    }
}

#[test]
fn property_writes_reject_non_finite_values_bad_scopes_and_null_pointers() {
    let world = new_world();
    let id = material(world);
    assert_ne!(
        set_float(world, SCOPE_MATERIAL, id, "_X", f32::INFINITY).code,
        0
    );
    assert_ne!(set_float(world, 7, id, "_X", 1.0).code, 0);
    assert_ne!(set_float(world, SCOPE_MATERIAL, id + 1, "_X", 1.0).code, 0);
    assert_ne!(set_float(world, SCOPE_MATERIAL, id, "", 1.0).code, 0);
    let (name, length) = text("_V");
    let vector = [1.0, f32::NAN, 0.0, 0.0];
    unsafe {
        let nan =
            disunity_shader_vector_write(world, SCOPE_MATERIAL, id, name, length, vector.as_ptr());
        assert_ne!(nan.code, 0);
        let null =
            disunity_shader_vector_write(world, SCOPE_MATERIAL, id, name, length, std::ptr::null());
        assert_ne!(null.code, 0);
    }
    assert_eq!(read_property(world, SCOPE_MATERIAL, id, "_X").present, 0);
}

#[test]
fn keywords_switch_on_a_material_and_in_the_global_state_but_not_a_block() {
    let world = new_world();
    let id = material(world);
    let (name, length) = text("FOG");
    let renderer = renderer(world);
    unsafe {
        assert_eq!(
            disunity_shader_keyword_write(world, SCOPE_MATERIAL, id, name, length, 1).code,
            0
        );
        assert_eq!(
            disunity_shader_keyword_write(world, SCOPE_GLOBAL, 0, name, length, 1).code,
            0
        );
        assert_ne!(
            disunity_shader_keyword_write(world, SCOPE_BLOCK, renderer, name, length, 1).code,
            0
        );
        assert_ne!(
            disunity_shader_keyword_write(world, SCOPE_GLOBAL, 0, name, length, 2).code,
            0
        );
        let mut on = 9;
        assert_eq!(
            disunity_shader_keyword_read(world, SCOPE_MATERIAL, id, name, length, &mut on).code,
            0
        );
        assert_eq!(on, 1);
        assert_eq!(
            disunity_shader_keyword_write(world, SCOPE_MATERIAL, id, name, length, 0).code,
            0
        );
        assert_eq!(
            disunity_shader_keyword_read(world, SCOPE_MATERIAL, id, name, length, &mut on).code,
            0
        );
        assert_eq!(on, 0);
        assert_eq!(
            disunity_shader_keyword_read(world, SCOPE_GLOBAL, 0, name, length, &mut on).code,
            0
        );
        assert_eq!(on, 1);
    }
}

#[test]
fn a_renderers_property_block_is_set_by_properties_and_cleared_by_one_call() {
    let world = new_world();
    let renderer = renderer(world);
    assert_eq!(
        set_float(world, SCOPE_BLOCK, renderer, "_Fade", 0.5).code,
        0
    );
    assert_eq!(
        disunity_shader_properties_clear(world, SCOPE_BLOCK, renderer).code,
        0
    );
    assert_eq!(
        read_property(world, SCOPE_BLOCK, renderer, "_Fade").present,
        0
    );
}

#[test]
fn clearing_properties_keeps_keywords_and_empties_materials_and_globals() {
    let world = new_world();
    let id = material(world);
    let (name, length) = text("FOG");
    unsafe {
        assert_eq!(
            disunity_shader_keyword_write(world, SCOPE_MATERIAL, id, name, length, 1).code,
            0
        );
    }
    for (scope, target) in [(SCOPE_MATERIAL, id), (SCOPE_GLOBAL, 0)] {
        assert_eq!(set_float(world, scope, target, "_Cutoff", 0.25).code, 0);
        assert_eq!(
            disunity_shader_properties_clear(world, scope, target).code,
            0
        );
        assert_eq!(read_property(world, scope, target, "_Cutoff").present, 0);
    }
    let mut on = 0;
    unsafe {
        assert_eq!(
            disunity_shader_keyword_read(world, SCOPE_MATERIAL, id, name, length, &mut on).code,
            0
        );
        assert_ne!(disunity_shader_properties_clear(world, 9, 0).code, 0);
    }
    assert_eq!(on, 1);
}

#[test]
fn renderer_slots_take_world_materials_and_reject_stale_ones_and_too_many() {
    let world = new_world();
    let renderer = renderer(world);
    let first = material(world);
    let mut value = unsafe { std::mem::zeroed::<DisunityMeshRenderer>() };
    unsafe {
        assert_eq!(
            disunity_mesh_renderer_read(world, renderer, &mut value).code,
            0
        )
    };
    value.material_count = 2;
    unsafe {
        assert_eq!(
            disunity_mesh_renderer_write(world, renderer, &value).code,
            0
        )
    };
    let slots = [first, 0];
    unsafe {
        let written = disunity_mesh_renderer_materials_write(world, renderer, slots.as_ptr(), 2);
        assert_eq!(written.code, 0);
        let too_many = [first; 3];
        let rejected =
            disunity_mesh_renderer_materials_write(world, renderer, too_many.as_ptr(), 3);
        assert_ne!(rejected.code, 0);
        let stale = [first + (1 << 32)];
        let rejected = disunity_mesh_renderer_materials_write(world, renderer, stale.as_ptr(), 1);
        assert_ne!(rejected.code, 0);
        let null = disunity_mesh_renderer_materials_write(world, renderer, std::ptr::null(), 1);
        assert_ne!(null.code, 0);
    }
    let mut count = 0;
    let mut read = [7_u64; 2];
    unsafe {
        let result = disunity_mesh_renderer_materials_read(
            world,
            renderer,
            read.as_mut_ptr(),
            2,
            &mut count,
        );
        assert_eq!(result.code, 0);
    }
    assert_eq!((count, read), (2, [first, 0]));
}

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

/// The fields a record carries are the fields it was recorded with.
fn assert_same_work(given: &DisunityOffscreenWork, read: &DisunityOffscreenWork) {
    assert_eq!(given.kind, read.kind);
    assert_eq!((given.camera, given.target), (read.camera, read.target));
    assert_eq!(
        (given.source_kind, given.source),
        (read.source_kind, read.source)
    );
    assert_eq!((given.material, given.pass), (read.material, read.pass));
    assert_eq!(
        (given.source_mip, given.target_mip),
        (read.source_mip, read.target_mip)
    );
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
    let mut count = 0;
    let mut taken = [DisunityOffscreenWork::default(); 4];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, std::ptr::null_mut(), 0, &mut count).code,
            0
        );
        assert_eq!(count, 3);
        let small = disunity_offscreen_take(world, taken.as_mut_ptr(), 2, &mut count);
        assert_ne!(small.code, 0);
        assert_eq!(
            disunity_offscreen_take(world, taken.as_mut_ptr(), 4, &mut count).code,
            0
        );
    }
    assert_eq!(count, 3);
    for (given, read) in records.iter().zip(&taken) {
        assert_same_work(given, read);
    }
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, taken.as_mut_ptr(), 4, &mut count).code,
            0
        )
    };
    assert_eq!(count, 0);
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
    let mut count = 9;
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, std::ptr::null_mut(), 0, &mut count).code,
            0
        )
    };
    assert_eq!(count, 0);
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
