//! The work command buffers record: validation, call order and the bytes that carry it.
use worldrunner_world_simulation::graphics::snapshot_wire::{
    WIRE_VERSION, WireError, decode, decode_work, encode, encode_work,
};
use worldrunner_world_simulation::graphics::{
    GraphicsError, GraphicsSnapshot, MAX_OFFSCREEN_WORK, OffscreenWork, PropertyValue,
    RenderTextureId, TargetRef, TextureRef, TextureSlot, WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn description() -> WorldRenderTexture {
    WorldRenderTexture {
        width: 64,
        height: 32,
        format: 8,
        depth_bits: 24,
        mip_count: 3,
        srgb: true,
        sampler: WorldSampler {
            filter: 2,
            ..WorldSampler::default()
        },
        generation: 0,
    }
}

fn texture(world: &mut World) -> RenderTextureId {
    world.create_render_texture(description()).unwrap()
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn set_target(color: Vec<TargetRef>, depth: Option<TargetRef>) -> OffscreenWork {
    OffscreenWork::SetRenderTarget {
        color,
        depth,
        mip: 0,
        face: -1,
        slice: -1,
    }
}

fn clear(value: [f32; 4], depth_value: f32) -> OffscreenWork {
    OffscreenWork::ClearRenderTarget {
        color: true,
        depth: true,
        stencil: false,
        value,
        depth_value,
        stencil_value: 7,
    }
}

/// One record of each command kind, naming the live texture `id`.
fn every_command(id: RenderTextureId) -> Vec<OffscreenWork> {
    let matrix = std::array::from_fn(|index| 0.5 * f32::from(u8::try_from(index).unwrap()) - 1.0);
    vec![
        OffscreenWork::AllocateTemporary {
            name_id: -4_000,
            desc: WorldRenderTexture {
                generation: 0,
                ..description()
            },
        },
        OffscreenWork::SetRenderTarget {
            color: vec![
                TargetRef::Render(id),
                TargetRef::Temporary(-4_000),
                TargetRef::CameraTarget,
            ],
            depth: Some(TargetRef::Temporary(12)),
            mip: 2,
            face: 5,
            slice: 3,
        },
        clear([0.25, -0.0, 1.0e-30, 3.0e30], 0.5),
        OffscreenWork::SetGlobal(PropertyValue::Float("_F".into(), 0.75)),
        OffscreenWork::SetGlobal(PropertyValue::Int("_I".into(), -9)),
        OffscreenWork::SetGlobal(PropertyValue::Vector("_V".into(), [1.0, 2.0, 3.0, 4.0])),
        OffscreenWork::SetGlobal(PropertyValue::Matrix("_M".into(), matrix)),
        OffscreenWork::SetGlobal(PropertyValue::Texture(
            "_T".into(),
            TextureSlot {
                texture: TextureRef::Render(id),
                scale: [2.0, 0.5],
                offset: [0.25, 0.75],
            },
        )),
        OffscreenWork::SetGlobalKeyword {
            name: "_FOG".into(),
            enabled: true,
        },
        OffscreenWork::SetGlobalKeyword {
            name: "_FOG".into(),
            enabled: false,
        },
        OffscreenWork::ReleaseTemporary { name_id: -4_000 },
    ]
}

#[test]
fn every_command_is_recorded_and_taken_in_call_order() {
    let mut world = world();
    let id = texture(&mut world);
    let records = every_command(id);
    for record in records.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.offscreen_work(), records.as_slice());
    assert_eq!(world.take_offscreen(), records);
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn a_command_is_not_folded_into_the_global_shader_state() {
    let mut world = world();
    world
        .record_offscreen(OffscreenWork::SetGlobal(PropertyValue::Float(
            "_Recorded".into(),
            1.0,
        )))
        .unwrap();
    world
        .record_offscreen(OffscreenWork::SetGlobalKeyword {
            name: "_RECORDED".into(),
            enabled: true,
        })
        .unwrap();
    let state = world.global_shader_state();
    assert!(state.properties.is_empty());
    assert!(state.keywords.is_empty());
    assert_eq!(state.generation, 0);
}

#[test]
fn commands_interleave_with_other_offscreen_work_in_call_order() {
    let mut world = world();
    let id = texture(&mut world);
    let blit = OffscreenWork::Blit {
        source: TextureRef::None,
        target: id,
        material: None,
        pass: -1,
    };
    let records = [
        set_target(vec![TargetRef::None], None),
        blit.clone(),
        clear([0.0; 4], 1.0),
        blit,
    ];
    for record in records.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.take_offscreen(), records);
}

#[test]
fn a_target_that_names_a_destroyed_render_texture_is_refused() {
    let mut world = world();
    let live = texture(&mut world);
    let dead = texture(&mut world);
    world.destroy_render_texture(dead).unwrap();
    let refused = [
        set_target(vec![TargetRef::Render(dead)], None),
        set_target(vec![TargetRef::Render(live)], Some(TargetRef::Render(dead))),
        OffscreenWork::SetGlobal(PropertyValue::Texture(
            "_T".into(),
            TextureSlot {
                texture: TextureRef::Render(dead),
                ..TextureSlot::default()
            },
        )),
    ];
    for record in refused {
        assert_eq!(
            graphics_error(world.record_offscreen(record)),
            GraphicsError::StaleRenderTexture
        );
    }
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn targets_of_a_name_the_world_does_not_track_are_accepted() {
    let mut world = world();
    let record = set_target(
        vec![TargetRef::Temporary(i32::MIN), TargetRef::Temporary(0)],
        Some(TargetRef::CameraTarget),
    );
    world.record_offscreen(record.clone()).unwrap();
    assert_eq!(world.take_offscreen(), [record]);
}

#[test]
fn a_target_list_holds_one_to_eight_colors_or_none_when_a_depth_is_named() {
    let mut world = world();
    let eight = vec![TargetRef::None; 8];
    world.record_offscreen(set_target(eight, None)).unwrap();
    let nine = vec![TargetRef::None; 9];
    assert_eq!(
        graphics_error(world.record_offscreen(set_target(nine, None))),
        GraphicsError::InvalidOffscreen
    );
    world
        .record_offscreen(set_target(Vec::new(), Some(TargetRef::CameraTarget)))
        .unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(set_target(Vec::new(), None))),
        GraphicsError::InvalidOffscreen
    );
}

#[test]
fn a_cube_face_is_minus_one_to_five_and_a_slice_is_minus_one_or_more() {
    let mut world = world();
    let record = |face, slice| OffscreenWork::SetRenderTarget {
        color: vec![TargetRef::None],
        depth: None,
        mip: 0,
        face,
        slice,
    };
    for (face, slice) in [(-1, -1), (0, 0), (5, 100)] {
        world.record_offscreen(record(face, slice)).unwrap();
    }
    for (face, slice) in [(-2, 0), (6, 0), (0, -2)] {
        assert_eq!(
            graphics_error(world.record_offscreen(record(face, slice))),
            GraphicsError::InvalidOffscreen,
            "face {face} slice {slice}"
        );
    }
}

#[test]
fn a_clear_with_a_value_that_is_not_finite_is_refused() {
    let mut world = world();
    world.record_offscreen(clear([0.0; 4], 1.0)).unwrap();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            graphics_error(world.record_offscreen(clear([0.0, bad, 0.0, 0.0], 1.0))),
            GraphicsError::NotFinite
        );
        assert_eq!(
            graphics_error(world.record_offscreen(clear([0.0; 4], bad))),
            GraphicsError::NotFinite
        );
    }
    assert_eq!(world.take_offscreen().len(), 1);
}

#[test]
fn global_values_need_a_name_and_finite_numbers() {
    let mut world = world();
    let refused = [
        (
            OffscreenWork::SetGlobal(PropertyValue::Float(String::new(), 1.0)),
            GraphicsError::InvalidName,
        ),
        (
            OffscreenWork::SetGlobal(PropertyValue::Float("_F".into(), f32::NAN)),
            GraphicsError::NotFinite,
        ),
        (
            OffscreenWork::SetGlobal(PropertyValue::Vector(
                "_V".into(),
                [0.0, 0.0, f32::INFINITY, 0.0],
            )),
            GraphicsError::NotFinite,
        ),
        (
            OffscreenWork::SetGlobal(PropertyValue::Texture(
                "_T".into(),
                TextureSlot {
                    texture: TextureRef::Global("_Other".into()),
                    ..TextureSlot::default()
                },
            )),
            GraphicsError::InvalidTexture,
        ),
        (
            OffscreenWork::SetGlobalKeyword {
                name: String::new(),
                enabled: true,
            },
            GraphicsError::InvalidName,
        ),
        (
            OffscreenWork::SetGlobalKeyword {
                name: "K".repeat(257),
                enabled: false,
            },
            GraphicsError::InvalidName,
        ),
    ];
    for (record, error) in refused {
        assert_eq!(graphics_error(world.record_offscreen(record)), error);
    }
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn a_temporary_is_allocated_with_a_description_the_world_accepts() {
    let mut world = world();
    let allocate = |desc| OffscreenWork::AllocateTemporary { name_id: 5, desc };
    world.record_offscreen(allocate(description())).unwrap();
    let bad = [
        WorldRenderTexture {
            width: 0,
            ..description()
        },
        WorldRenderTexture {
            height: 16_385,
            ..description()
        },
        WorldRenderTexture {
            depth_bits: 8,
            ..description()
        },
        WorldRenderTexture {
            mip_count: 16,
            ..description()
        },
    ];
    for desc in bad {
        assert_eq!(
            graphics_error(world.record_offscreen(allocate(desc))),
            GraphicsError::InvalidRenderTexture
        );
    }
    world
        .record_offscreen(OffscreenWork::ReleaseTemporary { name_id: 5 })
        .unwrap();
    assert_eq!(world.take_offscreen().len(), 2);
}

#[test]
fn commands_share_the_frame_cap() {
    let mut world = world();
    for _ in 0..MAX_OFFSCREEN_WORK {
        world
            .record_offscreen(OffscreenWork::ReleaseTemporary { name_id: 1 })
            .unwrap();
    }
    let over = world.record_offscreen(OffscreenWork::ReleaseTemporary { name_id: 1 });
    assert!(matches!(over, Err(WorldError::CapacityExceeded(_))));
    assert_eq!(world.take_offscreen().len(), MAX_OFFSCREEN_WORK);
}

fn snapshot_of(records: &[OffscreenWork], world: &mut World) -> GraphicsSnapshot {
    for record in records {
        world.record_offscreen(record.clone()).unwrap();
    }
    GraphicsSnapshot::take(world).unwrap()
}

#[test]
fn the_wire_version_is_two_and_every_command_round_trips() {
    assert_eq!(WIRE_VERSION, 2);
    let mut world = world();
    let id = texture(&mut world);
    let records = every_command(id);
    let snapshot = snapshot_of(&records, &mut world);
    assert_eq!(snapshot.work, records);
    let back = decode(&encode(&snapshot).unwrap()).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn each_command_kind_round_trips_on_its_own() {
    let mut world = world();
    let id = texture(&mut world);
    for record in every_command(id) {
        let snapshot = snapshot_of(std::slice::from_ref(&record), &mut world);
        let back = decode(&encode(&snapshot).unwrap()).unwrap();
        assert_eq!(back.work, [record]);
    }
}

#[test]
fn clear_values_keep_their_bits_on_the_wire() {
    let mut world = world();
    let record = OffscreenWork::ClearRenderTarget {
        color: false,
        depth: true,
        stencil: true,
        value: [-0.0, 1.0e-38, 0.1, 123_456.79],
        depth_value: f32::from_bits(1),
        stencil_value: u32::MAX,
    };
    let snapshot = snapshot_of(std::slice::from_ref(&record), &mut world);
    let back = decode(&encode(&snapshot).unwrap()).unwrap();
    let [
        OffscreenWork::ClearRenderTarget {
            color,
            depth,
            stencil,
            value,
            depth_value,
            stencil_value,
        },
    ] = back.work.as_slice()
    else {
        panic!("expected one clear, got {:?}", back.work);
    };
    assert_eq!((*color, *depth, *stencil), (false, true, true));
    assert_eq!(
        value.map(f32::to_bits),
        [
            (-0.0_f32).to_bits(),
            1.0e-38_f32.to_bits(),
            0.1_f32.to_bits(),
            123_456.79_f32.to_bits()
        ]
    );
    assert_eq!(depth_value.to_bits(), 1);
    assert_eq!(*stencil_value, u32::MAX);
}

/// A snapshot body that holds one record and nothing else: three empty maps, no global, the
/// work list of one, then `record`, and the empty remainder.
fn one_record(record: &[u8]) -> Vec<u8> {
    let mut body = vec![0; 12];
    body.push(0);
    body.extend_from_slice(&1_u32.to_le_bytes());
    body.extend_from_slice(record);
    body.extend_from_slice(&[0; 16]);
    let mut bytes = b"DGSN".to_vec();
    bytes.extend_from_slice(&WIRE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&body);
    bytes
}

#[test]
fn an_unknown_target_or_property_tag_is_refused_by_name() {
    // SetRenderTarget (kind 4) with one color whose target tag is 9.
    let mut target = vec![4];
    target.extend_from_slice(&1_u32.to_le_bytes());
    target.push(9);
    assert_eq!(decode(&one_record(&target)), Err(WireError::UnknownTag(9)));
    // SetGlobal (kind 6) with a property tag of 9.
    assert_eq!(decode(&one_record(&[6, 9])), Err(WireError::UnknownTag(9)));
}

#[test]
fn one_record_round_trips_without_a_snapshot_around_it() {
    let mut world = world();
    let id = texture(&mut world);
    for record in every_command(id) {
        let bytes = encode_work(&record).unwrap();
        assert_eq!(decode_work(&bytes).unwrap(), record);
        assert_eq!(
            decode_work(&bytes[..bytes.len() - 1]),
            Err(WireError::Truncated)
        );
        let mut longer = bytes;
        longer.push(0);
        assert_eq!(decode_work(&longer), Err(WireError::TrailingBytes));
    }
}
