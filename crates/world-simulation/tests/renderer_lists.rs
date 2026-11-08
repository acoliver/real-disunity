//! Renderer lists: the records a script's culling result produces, their validation, the draw
//! record that names a list, and the bytes that carry them.
use worldrunner_world_simulation::graphics::snapshot_wire::{WireError, decode_work, encode_work};
use worldrunner_world_simulation::graphics::{
    GraphicsError, MAX_NAME_BYTES, MaterialId, OffscreenWork, RendererListDesc, WorldShader,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, ObjectDesc, TimeSpan, World, WorldError, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn camera(world: &mut World) -> ComponentHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap()
}

fn material(world: &mut World) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Lit".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

fn cull(world: &mut World, id: u64) {
    let camera = camera(world);
    world
        .record_offscreen(OffscreenWork::Cull {
            id,
            camera,
            culling_mask: u32::MAX,
            planes: Vec::new(),
            options: 0,
        })
        .unwrap();
}

fn draw(cull: u64) -> RendererListDesc {
    RendererListDesc::Draw {
        cull,
        pass_tags: vec!["UniversalForward".into(), "SRPDefaultUnlit".into()],
        queue_min: 0,
        queue_max: 2500,
        layer_mask: 0xFFFF_FFFB,
        rendering_layer_mask: 0x0000_00FF,
        sorting: 0x1234,
        per_object: 0x56,
        override_material: None,
        override_pass: -1,
    }
}

fn shadow(cull: u64) -> RendererListDesc {
    RendererListDesc::Shadow {
        cull,
        light_index: 3,
        planes: vec![[0.0, 1.0, 0.0, -2.5], [1.0, 0.0, 0.0, 4.0]],
        sphere: [1.0, 2.0, 3.0, 40.0],
        objects_filter: 2,
        projection: 1,
    }
}

fn graphics_error<T: std::fmt::Debug>(result: Result<T, WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn with_draw(mut change: impl FnMut(&mut RendererListDesc)) -> RendererListDesc {
    let mut desc = draw(1);
    change(&mut desc);
    desc
}

fn with_shadow(mut change: impl FnMut(&mut RendererListDesc)) -> RendererListDesc {
    let mut desc = shadow(1);
    change(&mut desc);
    desc
}

fn set_tags(tags: Vec<String>) -> RendererListDesc {
    let mut desc = draw(1);
    if let RendererListDesc::Draw { pass_tags, .. } = &mut desc {
        *pass_tags = tags;
    }
    desc
}

#[test]
fn both_kinds_are_recorded_in_place_with_their_ids() {
    let mut world = world();
    cull(&mut world, 1);
    let first = world.create_renderer_list(draw(1)).unwrap();
    let second = world.create_renderer_list(shadow(1)).unwrap();
    assert_ne!(first, second);
    let work = world.offscreen_work();
    assert_eq!(work.len(), 3);
    assert_eq!(
        work[1],
        OffscreenWork::CreateRendererList {
            id: first,
            desc: draw(1)
        }
    );
    assert_eq!(
        work[2],
        OffscreenWork::CreateRendererList {
            id: second,
            desc: shadow(1)
        }
    );
}

#[test]
fn list_ids_are_nonzero_and_never_collide_with_culling_ids() {
    let mut world = world();
    cull(&mut world, 1);
    let mut ids = vec![1];
    for _ in 0..4 {
        ids.push(world.create_renderer_list(draw(1)).unwrap());
    }
    assert!(ids.iter().all(|id| *id != 0));
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
    // A culling id the lists already took is refused, and a list id is not a free culling id.
    let taken = ids[1];
    let camera = camera(&mut world);
    let result = world.record_offscreen(OffscreenWork::Cull {
        id: taken,
        camera,
        culling_mask: 1,
        planes: Vec::new(),
        options: 0,
    });
    assert_eq!(graphics_error(result), GraphicsError::InvalidOffscreen);
}

#[test]
fn a_list_needs_a_culling_result_recorded_this_frame() {
    let mut world = world();
    assert_eq!(
        graphics_error(world.create_renderer_list(draw(1))),
        GraphicsError::InvalidOffscreen
    );
    cull(&mut world, 1);
    let list = world.create_renderer_list(draw(1)).unwrap();
    // A list is not a culling result.
    assert_eq!(
        graphics_error(world.create_renderer_list(draw(list))),
        GraphicsError::InvalidOffscreen
    );
    assert_eq!(
        graphics_error(world.create_renderer_list(shadow(0))),
        GraphicsError::InvalidOffscreen
    );
    world.take_offscreen();
    assert_eq!(
        graphics_error(world.create_renderer_list(draw(1))),
        GraphicsError::InvalidOffscreen
    );
}

#[test]
fn every_draw_validation_failure_records_nothing() {
    let mut world = world();
    cull(&mut world, 1);
    let live = material(&mut world);
    let dead = material(&mut world);
    world.destroy_material(dead).unwrap();
    let name = |len: usize| "a".repeat(len);
    let bad = [
        set_tags(Vec::new()),
        set_tags((0..17).map(|n| format!("tag{n}")).collect()),
        set_tags(vec![String::new()]),
        set_tags(vec!["fine".into(), String::new()]),
        set_tags(vec![name(MAX_NAME_BYTES + 1)]),
        with_draw(|desc| {
            if let RendererListDesc::Draw {
                queue_min,
                queue_max,
                ..
            } = desc
            {
                *queue_min = 5;
                *queue_max = 4;
            }
        }),
        with_draw(|desc| {
            if let RendererListDesc::Draw {
                override_material, ..
            } = desc
            {
                *override_material = Some(dead);
            }
        }),
    ];
    for desc in bad {
        assert!(
            world.create_renderer_list(desc.clone()).is_err(),
            "{desc:?}"
        );
    }
    assert_eq!(world.offscreen_work().len(), 1);
    // The boundary values are accepted.
    let mut ok = set_tags((0..16).map(|n| name(MAX_NAME_BYTES - n)).collect());
    world.create_renderer_list(ok.clone()).unwrap();
    ok = with_draw(|desc| {
        if let RendererListDesc::Draw {
            queue_min,
            queue_max,
            override_material,
            ..
        } = desc
        {
            *queue_min = 7;
            *queue_max = 7;
            *override_material = Some(live);
        }
    });
    world.create_renderer_list(ok).unwrap();
    assert_eq!(world.offscreen_work().len(), 3);
}

#[test]
fn a_draw_tag_count_error_names_the_offscreen_error() {
    let mut world = world();
    cull(&mut world, 1);
    assert_eq!(
        graphics_error(world.create_renderer_list(set_tags(Vec::new()))),
        GraphicsError::InvalidOffscreen
    );
    assert_eq!(
        graphics_error(world.create_renderer_list(set_tags(vec![String::new()]))),
        GraphicsError::InvalidName
    );
}

#[test]
fn every_shadow_validation_failure_records_nothing() {
    let mut world = world();
    cull(&mut world, 1);
    let bad = [
        with_shadow(|desc| {
            if let RendererListDesc::Shadow { light_index, .. } = desc {
                *light_index = -1;
            }
        }),
        with_shadow(|desc| {
            if let RendererListDesc::Shadow { planes, .. } = desc {
                *planes = vec![[0.0; 4]; 11];
            }
        }),
        with_shadow(|desc| {
            if let RendererListDesc::Shadow { objects_filter, .. } = desc {
                *objects_filter = 3;
            }
        }),
        with_shadow(|desc| {
            if let RendererListDesc::Shadow { projection, .. } = desc {
                *projection = 3;
            }
        }),
        with_shadow(|desc| {
            if let RendererListDesc::Shadow { planes, .. } = desc {
                planes[1][2] = f32::NAN;
            }
        }),
        with_shadow(|desc| {
            if let RendererListDesc::Shadow { sphere, .. } = desc {
                sphere[3] = f32::INFINITY;
            }
        }),
    ];
    for desc in bad {
        assert!(
            world.create_renderer_list(desc.clone()).is_err(),
            "{desc:?}"
        );
    }
    assert_eq!(world.offscreen_work().len(), 1);
    let limit = with_shadow(|desc| {
        if let RendererListDesc::Shadow {
            planes,
            light_index,
            objects_filter,
            projection,
            ..
        } = desc
        {
            *planes = vec![[0.0, 1.0, 0.0, 1.0]; 10];
            *light_index = 0;
            *objects_filter = 2;
            *projection = 2;
        }
    });
    world.create_renderer_list(limit).unwrap();
    assert_eq!(world.offscreen_work().len(), 2);
}

#[test]
fn shadow_errors_name_what_is_wrong() {
    let mut world = world();
    cull(&mut world, 1);
    let nan = with_shadow(|desc| {
        if let RendererListDesc::Shadow { sphere, .. } = desc {
            sphere[0] = f32::NAN;
        }
    });
    assert_eq!(
        graphics_error(world.create_renderer_list(nan)),
        GraphicsError::NotFinite
    );
    let many = with_shadow(|desc| {
        if let RendererListDesc::Shadow { planes, .. } = desc {
            *planes = vec![[0.0; 4]; 11];
        }
    });
    assert_eq!(
        graphics_error(world.create_renderer_list(many)),
        GraphicsError::InvalidOffscreen
    );
}

#[test]
fn a_destroyed_override_material_is_stale() {
    let mut world = world();
    cull(&mut world, 1);
    let dead = material(&mut world);
    world.destroy_material(dead).unwrap();
    let desc = with_draw(|desc| {
        if let RendererListDesc::Draw {
            override_material, ..
        } = desc
        {
            *override_material = Some(dead);
        }
    });
    assert_eq!(
        graphics_error(world.create_renderer_list(desc)),
        GraphicsError::StaleMaterial
    );
}

#[test]
fn record_offscreen_refuses_a_list_creation() {
    let mut world = world();
    cull(&mut world, 1);
    let result = world.record_offscreen(OffscreenWork::CreateRendererList {
        id: 9,
        desc: draw(1),
    });
    assert_eq!(graphics_error(result), GraphicsError::InvalidOffscreen);
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn drawing_a_list_needs_a_list_created_this_frame() {
    let mut world = world();
    cull(&mut world, 1);
    let list = world.create_renderer_list(draw(1)).unwrap();
    for unknown in [0, 1, list + 100] {
        let result = world.record_offscreen(OffscreenWork::DrawRendererList { list: unknown });
        assert_eq!(graphics_error(result), GraphicsError::InvalidOffscreen);
    }
    assert_eq!(world.offscreen_work().len(), 2);
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list })
        .unwrap();
    assert_eq!(
        world.offscreen_work()[2],
        OffscreenWork::DrawRendererList { list }
    );
    assert_eq!(world.offscreen_work().len(), 4);
}

#[test]
fn a_list_of_an_earlier_frame_cannot_be_drawn() {
    let mut world = world();
    cull(&mut world, 1);
    let taken_list = world.create_renderer_list(shadow(1)).unwrap();
    world.take_offscreen();
    let result = world.record_offscreen(OffscreenWork::DrawRendererList { list: taken_list });
    assert_eq!(graphics_error(result), GraphicsError::InvalidOffscreen);

    cull(&mut world, 1);
    let dropped_list = world.create_renderer_list(draw(1)).unwrap();
    world
        .advance(TimeSpan::from_nanos(16_000_000).unwrap())
        .unwrap();
    assert!(world.offscreen_work().is_empty());
    let result = world.record_offscreen(OffscreenWork::DrawRendererList { list: dropped_list });
    assert_eq!(graphics_error(result), GraphicsError::InvalidOffscreen);
    // The frame's reset frees the culling id and the list id for reuse.
    cull(&mut world, 1);
    world.create_renderer_list(draw(1)).unwrap();
}

#[test]
fn both_kinds_and_the_draw_survive_the_wire() {
    let material = material(&mut world());
    let records = [
        OffscreenWork::CreateRendererList {
            id: 5,
            desc: draw(2),
        },
        OffscreenWork::CreateRendererList {
            id: u64::MAX,
            desc: with_draw(|desc| {
                if let RendererListDesc::Draw {
                    override_material,
                    override_pass,
                    ..
                } = desc
                {
                    *override_material = Some(material);
                    *override_pass = 3;
                }
            }),
        },
        OffscreenWork::CreateRendererList {
            id: 6,
            desc: shadow(2),
        },
        OffscreenWork::CreateRendererList {
            id: 7,
            desc: RendererListDesc::Shadow {
                cull: 1,
                light_index: 0,
                planes: Vec::new(),
                sphere: [-0.0, f32::MIN_POSITIVE, 1.0e30, 2.0],
                objects_filter: 0,
                projection: 0,
            },
        },
        OffscreenWork::DrawRendererList { list: 6 },
    ];
    for record in records {
        let bytes = encode_work(&record).unwrap();
        assert_eq!(decode_work(&bytes).unwrap(), record);
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(decode_work(&longer), Err(WireError::TrailingBytes));
        for cut in 0..bytes.len() {
            assert!(decode_work(&bytes[..cut]).is_err());
        }
    }
}

#[test]
fn the_wire_refuses_lists_over_the_limits() {
    let too_many_tags = OffscreenWork::CreateRendererList {
        id: 1,
        desc: set_tags((0..17).map(|n| format!("t{n}")).collect()),
    };
    assert!(encode_work(&too_many_tags).is_err());
    let too_many_planes = OffscreenWork::CreateRendererList {
        id: 1,
        desc: with_shadow(|desc| {
            if let RendererListDesc::Shadow { planes, .. } = desc {
                *planes = vec![[0.0; 4]; 11];
            }
        }),
    };
    assert!(encode_work(&too_many_planes).is_err());
    // A variant tag of the list record that no encoder writes is refused.
    let mut bytes = encode_work(&OffscreenWork::CreateRendererList {
        id: 1,
        desc: shadow(1),
    })
    .unwrap();
    bytes[9] = 9;
    assert!(matches!(decode_work(&bytes), Err(WireError::UnknownTag(9))));
}
