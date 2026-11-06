//! The byte encoding of a graphics snapshot: it round-trips every field and refuses bad input.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::snapshot_wire::{
    MAX_SNAPSHOT_BYTES, WIRE_VERSION, WireError, decode, encode,
};
use worldrunner_world_simulation::graphics::{
    ComputeKernel, ComputeShaderId, GraphicsSnapshot, MaterialEdit, MaterialId, OffscreenWork,
    PropertyValue, RenderTextureId, ShaderSource, TextureRef, TextureSlot, WorldComputeShader,
    WorldSampler,
};
use worldrunner_world_simulation::presentation::{Camera, CameraClear, Projection};
use worldrunner_world_simulation::{MeshRenderer, World};

mod graphics_world;
use graphics_world::{
    camera, material, render_texture, renderer, renderer_with, shader, snapshot, world,
};

fn slot(texture: TextureRef) -> TextureSlot {
    TextureSlot {
        texture,
        scale: [2.0, 0.5],
        offset: [-1.25, 3.5e-9],
    }
}

/// Every kind of property value, with floats whose bit patterns a lossy encoding would change.
fn every_property(texture: TextureRef) -> [PropertyValue; 5] {
    let matrix = std::array::from_fn(|index| 0.1 * f32::from(u8::try_from(index).unwrap()) - 0.3);
    [
        PropertyValue::Float("_Float".into(), f32::MIN_POSITIVE),
        PropertyValue::Int("_Int".into(), -7),
        PropertyValue::Vector("_Vector".into(), [1.0 / 3.0, -0.0, 1.0e-30, 3.0e30]),
        PropertyValue::Matrix("_Matrix".into(), matrix),
        PropertyValue::Texture("_Texture".into(), slot(texture)),
    ]
}

fn edit_all(world: &mut World, id: MaterialId, values: [PropertyValue; 5]) {
    for value in values {
        world
            .edit_material(id, MaterialEdit::Property(value))
            .unwrap();
    }
}

/// A world whose frame holds a camera render, a blit with pass -1, a copy and a dispatch, the
/// materials with all five property kinds, a property block, a render texture sampler, global
/// state and renderers, and the snapshot taken from it.
fn full_snapshot() -> GraphicsSnapshot {
    let mut world = world();
    let target = render_texture(&mut world, [16, 8]);
    let other = render_texture(&mut world, [4, 4]);
    let settings = Camera {
        projection: Projection::Perspective,
        clear: CameraClear::DepthOnly,
        background: [0.1, 0.2, 0.3, 0.4],
        depth: -3,
        culling_mask: 0b1_0001,
        viewport: [0.25, 0.0, 0.5, 1.0],
        target_texture: Some(target),
        projection_matrix: Some([0.5; 16]),
        world_to_camera: Some([0.25; 16]),
        ..Camera::default()
    };
    let rotation = Quat::from_rotation_y(0.7);
    let camera = camera(&mut world, settings, Vec3::new(1.0, 2.0, 3.0), rotation);
    let blit_shader = shader(&mut world, "Blit");
    let blit = material(&mut world, blit_shader);
    edit_blit_material(&mut world, blit, other);
    for value in every_property(TextureRef::Asset((1 << 40) + 5)) {
        world.set_global_property(value).unwrap();
    }
    world.set_global_keyword("GLOBAL_ON", true).unwrap();
    add_renderers(&mut world, blit);
    record_work(&mut world, camera, target, blit, other);
    snapshot(&mut world)
}

fn edit_blit_material(world: &mut World, blit: MaterialId, other: RenderTextureId) {
    edit_all(world, blit, every_property(TextureRef::Render(other)));
    let edits = [
        MaterialEdit::Keyword {
            name: "FAST".into(),
            enabled: true,
        },
        MaterialEdit::Pass {
            name: "Shadow".into(),
            enabled: false,
        },
        MaterialEdit::RenderQueue(3000),
    ];
    for edit in edits {
        world.edit_material(blit, edit).unwrap();
    }
}

/// A renderer with a property block, a queue and an empty slot, and one on another layer.
fn add_renderers(world: &mut World, blit: MaterialId) {
    let mut properties = worldrunner_world_simulation::graphics::ShaderProperties::default();
    for value in every_property(TextureRef::None) {
        properties.set(value).unwrap();
    }
    let block = MeshRenderer {
        property_block: Some(properties),
        render_queue: Some(2450),
        material_count: 2,
        slot_materials: vec![Some(blit), None],
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), (1 << 41) + 1, block);
    renderer(world, 4, (1 << 41) + 2, vec![Some(blit)]);
}

/// A compute shader with a sized and an unsized kernel, a buffer bound to the first and every
/// kind of value set on it.
fn readback_shader(world: &mut World, other: RenderTextureId) -> ComputeShaderId {
    let compute = world
        .create_compute_shader(WorldComputeShader {
            name: "Readback".into(),
            source: ShaderSource {
                file: "resources.assets".into(),
                path_id: -8167,
            },
            kernels: vec![
                ComputeKernel {
                    name: "Sized".into(),
                    thread_group_size: Some([8, 4, 2]),
                },
                ComputeKernel {
                    name: "Unsized".into(),
                    thread_group_size: None,
                },
            ],
            generation: 0,
        })
        .unwrap();
    let buffer = world.create_compute_buffer(2, 8, 5).unwrap();
    world
        .set_compute_buffer_data(buffer, 3, &[0xDE, 0xAD, 0xBE, 0xEF])
        .unwrap();
    world
        .bind_compute_buffer(compute, 0, "_Output".into(), buffer)
        .unwrap();
    for value in every_property(TextureRef::Render(other)) {
        world.set_compute_property(compute, Some(0), value).unwrap();
    }
    compute
}

fn record_work(
    world: &mut World,
    camera: worldrunner_world_simulation::ComponentHandle,
    target: RenderTextureId,
    blit: MaterialId,
    other: RenderTextureId,
) {
    let compute = readback_shader(world, other);
    let records = [
        OffscreenWork::CameraRender { camera, target },
        OffscreenWork::Blit {
            source: TextureRef::Asset((1 << 40) + 9),
            target,
            material: Some(blit),
            pass: -1,
        },
        OffscreenWork::Blit {
            source: TextureRef::None,
            target: other,
            material: None,
            pass: 2,
        },
        OffscreenWork::CopyTexture {
            source: TextureRef::Render(other),
            source_element: 0,
            source_mip: 0,
            target,
            target_element: 0,
            target_mip: 0,
        },
    ];
    for record in records {
        world.record_offscreen(record).unwrap();
    }
    world.dispatch_compute(compute, 0, [64, 32, 1]).unwrap();
}

#[test]
fn a_snapshot_with_every_variant_and_field_round_trips_exactly() {
    let snapshot = full_snapshot();
    assert_eq!(snapshot.work.len(), 5);
    assert!(snapshot.global.is_some() && !snapshot.cameras.is_empty());
    assert!(!snapshot.renderers.is_empty() && !snapshot.compute_buffers.is_empty());
    let bytes = encode(&snapshot).unwrap();
    assert_eq!(decode(&bytes).unwrap(), snapshot);
}

#[test]
fn floats_keep_their_bits_including_negative_zero_and_nan_payloads() {
    let mut snapshot = full_snapshot();
    let global = snapshot.global.as_mut().unwrap();
    global.properties.floats[0].1 = f32::from_bits(0x7FC0_1234);
    global.properties.vectors[0].1 = [-0.0, f32::INFINITY, f32::NEG_INFINITY, f32::from_bits(1)];
    let view = snapshot.cameras.values_mut().next().unwrap();
    view.settings.background = [f64::from_bits(0x7FF8_0000_0000_0042), -0.0, 1.0e-300, 0.0];
    let back = decode(&encode(&snapshot).unwrap()).unwrap();
    let global = back.global.as_ref().unwrap();
    assert_eq!(global.properties.floats[0].1.to_bits(), 0x7FC0_1234);
    let vector = global.properties.vectors[0].1;
    assert_eq!(
        vector.map(f32::to_bits),
        [
            (-0.0_f32).to_bits(),
            f32::INFINITY.to_bits(),
            f32::NEG_INFINITY.to_bits(),
            1
        ]
    );
    let background = back.cameras.values().next().unwrap().settings.background;
    assert_eq!(
        background.map(f64::to_bits),
        [
            0x7FF8_0000_0000_0042,
            (-0.0_f64).to_bits(),
            1.0e-300_f64.to_bits(),
            0
        ]
    );
}

#[test]
fn an_empty_snapshot_round_trips() {
    let empty = GraphicsSnapshot::default();
    assert_eq!(decode(&encode(&empty).unwrap()).unwrap(), empty);
}

#[test]
fn a_render_texture_sampler_round_trips() {
    let mut snapshot = full_snapshot();
    let sampler = WorldSampler {
        wrap_u: 1,
        wrap_v: 2,
        wrap_w: 3,
        filter: 2,
        aniso_level: 9,
        mip_bias: -0.75,
    };
    snapshot
        .render_textures
        .values_mut()
        .next()
        .unwrap()
        .sampler = sampler;
    let back = decode(&encode(&snapshot).unwrap()).unwrap();
    assert_eq!(
        back.render_textures.values().next().unwrap().sampler,
        sampler
    );
}

#[test]
fn input_cut_short_at_any_byte_is_refused_as_truncated() {
    let bytes = encode(&full_snapshot()).unwrap();
    for length in 0..bytes.len() {
        assert_eq!(
            decode(&bytes[..length]),
            Err(WireError::Truncated),
            "cut at {length}"
        );
    }
}

#[test]
fn bytes_after_the_snapshot_are_refused() {
    let mut bytes = encode(&full_snapshot()).unwrap();
    bytes.push(0);
    assert_eq!(decode(&bytes), Err(WireError::TrailingBytes));
}

#[test]
fn another_marker_or_version_is_refused_by_name() {
    let bytes = encode(&GraphicsSnapshot::default()).unwrap();
    let mut marker = bytes.clone();
    marker[0] = b'X';
    assert_eq!(decode(&marker), Err(WireError::BadMarker));
    for version in [0, WIRE_VERSION + 1, u32::MAX] {
        let mut other = bytes.clone();
        other[4..8].copy_from_slice(&version.to_le_bytes());
        assert_eq!(decode(&other), Err(WireError::UnknownVersion(version)));
    }
}

#[test]
fn a_body_length_over_the_cap_is_refused_before_anything_is_read() {
    let mut bytes = encode(&GraphicsSnapshot::default()).unwrap();
    bytes[8..12].copy_from_slice(&(MAX_SNAPSHOT_BYTES + 1).to_le_bytes());
    assert_eq!(decode(&bytes), Err(WireError::TooLong));
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode(&bytes), Err(WireError::TooLong));
}

#[test]
fn a_body_length_that_disagrees_with_the_input_is_refused() {
    let bytes = encode(&GraphicsSnapshot::default()).unwrap();
    let body = u32::try_from(bytes.len() - 12).unwrap();
    let mut longer = bytes.clone();
    longer[8..12].copy_from_slice(&(body + 1).to_le_bytes());
    assert_eq!(decode(&longer), Err(WireError::Truncated));
    let mut shorter = bytes;
    shorter[8..12].copy_from_slice(&(body - 1).to_le_bytes());
    assert_eq!(decode(&shorter), Err(WireError::TrailingBytes));
}

#[test]
fn a_body_longer_than_the_snapshot_it_holds_is_refused() {
    let mut bytes = encode(&GraphicsSnapshot::default()).unwrap();
    bytes.push(0);
    let body = u32::try_from(bytes.len() - 12).unwrap();
    bytes[8..12].copy_from_slice(&body.to_le_bytes());
    assert_eq!(decode(&bytes), Err(WireError::TrailingBytes));
}

/// The bytes of a snapshot whose only content is `work`, so a test can corrupt one field.
fn with_header(body: &[u8]) -> Vec<u8> {
    let mut bytes = b"DGSN".to_vec();
    bytes.extend_from_slice(&WIRE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(body);
    bytes
}

/// Seven empty maps and lists, then `tail`: shaders, materials, render textures, no global.
fn empty_until_work(tail: &[u8]) -> Vec<u8> {
    let mut body = vec![0; 4 * 3];
    body.push(0);
    body.extend_from_slice(tail);
    with_header(&body)
}

#[test]
fn a_count_over_the_cap_or_longer_than_the_input_is_refused() {
    let huge = u32::MAX.to_le_bytes();
    assert_eq!(decode(&empty_until_work(&huge)), Err(WireError::TooLong));
    let beyond_input = 1_000_u32.to_le_bytes();
    assert_eq!(
        decode(&empty_until_work(&beyond_input)),
        Err(WireError::Truncated)
    );
}

#[test]
fn an_unknown_work_kind_is_refused_by_name() {
    let mut tail = 1_u32.to_le_bytes().to_vec();
    tail.push(9);
    assert_eq!(
        decode(&empty_until_work(&tail)),
        Err(WireError::UnknownTag(9))
    );
}

#[test]
fn text_that_is_not_utf8_is_refused() {
    let mut snapshot = full_snapshot();
    snapshot.shaders.values_mut().next().unwrap().name = "AB".into();
    let mut bytes = encode(&snapshot).unwrap();
    let at = bytes.windows(2).position(|pair| pair == b"AB").unwrap();
    bytes[at] = 0xFF;
    assert_eq!(decode(&bytes), Err(WireError::BadText));
}

#[test]
fn a_map_that_holds_a_key_twice_is_refused() {
    // The first map holds shaders: a key, a name and no source, twice with the same key.
    let mut shaders = 2_u32.to_le_bytes().to_vec();
    for name in ["A", "B"] {
        shaders.extend_from_slice(&(1_u64 << 32 | 1).to_le_bytes());
        shaders.extend_from_slice(&1_u32.to_le_bytes());
        shaders.extend_from_slice(name.as_bytes());
        shaders.push(0);
    }
    assert_eq!(decode(&with_header(&shaders)), Err(WireError::DuplicateKey));
}
