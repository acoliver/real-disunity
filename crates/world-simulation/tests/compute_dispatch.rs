//! Compute bindings and recorded dispatches: what a dispatch captures and where it sits among
//! the frame's other offscreen work.
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, ComputeKernel, ComputeShaderId, GraphicsError, MaterialId, OffscreenWork,
    PropertyValue, RenderTextureId, ShaderSource, TextureRef, TextureSlot, WorldComputeShader,
    WorldRenderTexture, WorldSampler, WorldShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn compute(world: &mut World) -> ComputeShaderId {
    let kernel = |name: &str| ComputeKernel {
        name: name.into(),
        thread_group_size: Some([8, 8, 1]),
    };
    world
        .create_compute_shader(WorldComputeShader {
            name: "Readback".into(),
            source: ShaderSource {
                file: "resources.assets".into(),
                path_id: 8167,
            },
            kernels: vec![kernel("A"), kernel("B")],
            generation: 0,
        })
        .unwrap()
}

fn graphics_error<T: std::fmt::Debug>(result: Result<T, WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn int(name: &str, value: i32) -> PropertyValue {
    PropertyValue::Int(name.into(), value)
}

fn global_slot(name: &str) -> PropertyValue {
    PropertyValue::Texture(
        "_Heights".into(),
        TextureSlot {
            texture: TextureRef::Global(name.into()),
            ..TextureSlot::default()
        },
    )
}

fn global_texture(name: &str, texture: TextureRef) -> PropertyValue {
    PropertyValue::Texture(
        name.into(),
        TextureSlot {
            texture,
            ..TextureSlot::default()
        },
    )
}

fn render_texture(world: &mut World) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: 4,
            height: 4,
            format: 8,
            depth_bits: 0,
            mip_count: 1,
            srgb: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn dispatched(world: &mut World) -> Vec<OffscreenWork> {
    world.take_offscreen()
}

#[test]
fn shader_wide_values_and_kernel_values_merge_into_one_snapshot() {
    let mut world = world();
    let id = compute(&mut world);
    let buffer = world.create_compute_buffer(4, 4, 0).unwrap();
    world
        .set_compute_property(id, None, int("_Width", 2048))
        .unwrap();
    world
        .set_compute_property(id, None, PropertyValue::Float("_Scale".into(), 0.5))
        .unwrap();
    world
        .set_compute_property(id, Some(1), int("_Width", 64))
        .unwrap();
    world
        .bind_compute_buffer(id, 1, "_Output".into(), buffer)
        .unwrap();

    let first = world.compute_bindings(id, 0).unwrap();
    assert_eq!(first.properties.int("_Width"), Some(2048));
    assert_eq!(first.properties.float("_Scale"), Some(0.5));
    assert!(first.buffers.is_empty());

    let second = world.compute_bindings(id, 1).unwrap();
    assert_eq!(second.properties.int("_Width"), Some(64));
    assert_eq!(second.properties.float("_Scale"), Some(0.5));
    assert_eq!(second.buffers, vec![("_Output".to_owned(), buffer)]);
}

#[test]
fn a_kernel_value_wins_over_the_shader_wide_value_of_the_same_name_in_every_kind() {
    let mut world = world();
    let id = compute(&mut world);
    let identity = |scale: f32| {
        let mut matrix = [0.0; 16];
        matrix[0] = scale;
        matrix
    };
    let slot = |texture: TextureRef| TextureSlot {
        texture,
        ..TextureSlot::default()
    };
    let values = |float: f32, vector: f32, matrix: f32, texture: TextureRef| {
        [
            PropertyValue::Float("_F".into(), float),
            PropertyValue::Vector("_V".into(), [vector; 4]),
            PropertyValue::Matrix("_M".into(), identity(matrix)),
            PropertyValue::Texture("_T".into(), slot(texture)),
        ]
    };
    for value in values(1.0, 1.0, 1.0, TextureRef::Asset(1 << 40)) {
        world.set_compute_property(id, None, value).unwrap();
    }
    world
        .set_compute_property(id, None, PropertyValue::Float("_OnlyShared".into(), 9.0))
        .unwrap();
    for value in values(2.0, 2.0, 2.0, TextureRef::Global("_G".into())) {
        world.set_compute_property(id, Some(0), value).unwrap();
    }
    world
        .set_compute_property(id, Some(0), PropertyValue::Float("_OnlyKernel".into(), 7.0))
        .unwrap();

    let kernel = world.compute_bindings(id, 0).unwrap().properties;
    assert_eq!(kernel.float("_F"), Some(2.0));
    assert_eq!(kernel.vector("_V"), Some([2.0; 4]));
    assert_eq!(kernel.matrix("_M"), Some(identity(2.0)));
    assert_eq!(
        kernel.texture("_T"),
        Some(slot(TextureRef::Global("_G".into())))
    );
    assert_eq!(kernel.float("_OnlyShared"), Some(9.0));
    assert_eq!(kernel.float("_OnlyKernel"), Some(7.0));

    let other = world.compute_bindings(id, 1).unwrap().properties;
    assert_eq!(other.float("_F"), Some(1.0));
    assert_eq!(other.vector("_V"), Some([1.0; 4]));
    assert_eq!(other.matrix("_M"), Some(identity(1.0)));
    assert_eq!(other.texture("_T"), Some(slot(TextureRef::Asset(1 << 40))));
    assert_eq!(other.float("_OnlyKernel"), None);
}

#[test]
fn binding_a_buffer_again_under_a_name_replaces_it() {
    let mut world = world();
    let id = compute(&mut world);
    let a = world.create_compute_buffer(1, 4, 0).unwrap();
    let b = world.create_compute_buffer(1, 4, 0).unwrap();
    world
        .bind_compute_buffer(id, 0, "_Output".into(), a)
        .unwrap();
    world
        .bind_compute_buffer(id, 0, "_Output".into(), b)
        .unwrap();
    world
        .bind_compute_buffer(id, 0, "_Input".into(), a)
        .unwrap();
    let bound = world.compute_bindings(id, 0).unwrap().buffers;
    assert_eq!(
        bound,
        vec![("_Input".to_owned(), a), ("_Output".to_owned(), b)]
    );
}

#[test]
fn bindings_check_the_kernel_the_buffer_and_the_name() {
    let mut world = world();
    let id = compute(&mut world);
    let buffer = world.create_compute_buffer(1, 4, 0).unwrap();
    world.destroy_compute_buffer(buffer).unwrap();
    assert_eq!(
        graphics_error(world.bind_compute_buffer(id, 0, "_Output".into(), buffer)),
        GraphicsError::StaleComputeBuffer
    );
    let live = world.create_compute_buffer(1, 4, 0).unwrap();
    assert_eq!(
        graphics_error(world.bind_compute_buffer(id, 2, "_Output".into(), live)),
        GraphicsError::InvalidKernel
    );
    assert_eq!(
        graphics_error(world.bind_compute_buffer(id, 0, String::new(), live)),
        GraphicsError::InvalidName
    );
    assert_eq!(
        graphics_error(world.set_compute_property(id, Some(2), int("_X", 1))),
        GraphicsError::InvalidKernel
    );
    assert_eq!(
        graphics_error(world.compute_bindings(id, 2)),
        GraphicsError::InvalidKernel
    );
    assert_eq!(
        graphics_error(world.set_compute_property(
            id,
            None,
            PropertyValue::Float("_Scale".into(), f32::NAN)
        )),
        GraphicsError::NotFinite
    );
}

#[test]
fn a_kernel_texture_may_name_a_global_texture() {
    let mut world = world();
    let id = compute(&mut world);
    world
        .set_compute_property(id, Some(0), global_slot("_GlobalTerrainHeightmap"))
        .unwrap();
    let bindings = world.compute_bindings(id, 0).unwrap();
    let slot = bindings.properties.texture("_Heights").unwrap();
    assert_eq!(
        slot.texture,
        TextureRef::Global("_GlobalTerrainHeightmap".into())
    );
    assert!(world.compute_bindings(id, 1).unwrap().properties.is_empty());
}

#[test]
fn a_kernel_texture_may_name_an_asset_a_render_texture_or_nothing() {
    let mut world = world();
    let id = compute(&mut world);
    let target = render_texture(&mut world);
    for texture in [
        TextureRef::Asset(5),
        TextureRef::Render(target),
        TextureRef::None,
    ] {
        let slot = TextureSlot {
            texture: texture.clone(),
            ..TextureSlot::default()
        };
        world
            .set_compute_property(id, Some(0), PropertyValue::Texture("_T".into(), slot))
            .unwrap();
        let held = world.compute_bindings(id, 0).unwrap();
        assert_eq!(held.properties.texture("_T").unwrap().texture, texture);
    }
    world.destroy_render_texture(target).unwrap();
    let slot = TextureSlot {
        texture: TextureRef::Render(target),
        ..TextureSlot::default()
    };
    assert!(
        world
            .set_compute_property(id, Some(0), PropertyValue::Texture("_T".into(), slot))
            .is_err()
    );
}

#[test]
fn a_global_texture_needs_a_name_and_only_compute_bindings_take_one() {
    let mut world = world();
    let id = compute(&mut world);
    assert_eq!(
        graphics_error(world.set_compute_property(id, Some(0), global_slot(""))),
        GraphicsError::InvalidName
    );
    let property = global_slot("_GlobalTerrainHeightmap");
    assert_eq!(
        graphics_error(world.set_global_property(property.clone())),
        GraphicsError::InvalidTexture
    );
    let shader = world
        .create_shader(WorldShader {
            name: "S".into(),
            source: None,
        })
        .unwrap();
    let material = world.create_material(shader).unwrap();
    let edit = worldrunner_world_simulation::graphics::MaterialEdit::Property(property);
    assert_eq!(
        graphics_error(world.edit_material(material, edit)),
        GraphicsError::InvalidTexture
    );
}

#[test]
fn a_dispatch_records_the_shader_kernel_groups_and_a_snapshot_of_the_bindings() {
    let mut world = world();
    let id = compute(&mut world);
    let buffer = world.create_compute_buffer(4, 4, 0).unwrap();
    world
        .set_compute_property(id, None, int("_OutputWidth", 2048))
        .unwrap();
    world
        .set_compute_property(id, Some(0), global_slot("_GlobalTerrainHeightmap"))
        .unwrap();
    world
        .bind_compute_buffer(id, 0, "_Output".into(), buffer)
        .unwrap();
    world
        .set_global_property(global_texture(
            "_GlobalTerrainHeightmap",
            TextureRef::Asset(1 << 40),
        ))
        .unwrap();
    world.dispatch_compute(id, 0, [256, 128, 1]).unwrap();

    let work = dispatched(&mut world);
    assert_eq!(work.len(), 1);
    let OffscreenWork::Dispatch {
        shader,
        kernel,
        groups,
        bindings,
    } = &work[0]
    else {
        panic!("expected a dispatch, got {:?}", work[0]);
    };
    assert_eq!((*shader, *kernel, *groups), (id, 0, [256, 128, 1]));
    assert_eq!(bindings.properties.int("_OutputWidth"), Some(2048));
    assert_eq!(bindings.buffers, vec![("_Output".to_owned(), buffer)]);
    assert_eq!(
        bindings.properties.texture("_Heights").unwrap().texture,
        TextureRef::Asset(1 << 40),
        "the record holds the global texture as it stood at the dispatch"
    );
}

#[test]
fn later_edits_do_not_change_a_recorded_dispatch() {
    let mut world = world();
    let id = compute(&mut world);
    let first = world.create_compute_buffer(1, 4, 0).unwrap();
    let second = world.create_compute_buffer(1, 4, 0).unwrap();
    world.set_compute_property(id, None, int("_N", 1)).unwrap();
    world
        .bind_compute_buffer(id, 0, "_Output".into(), first)
        .unwrap();
    world.dispatch_compute(id, 0, [1, 1, 1]).unwrap();

    world.set_compute_property(id, None, int("_N", 2)).unwrap();
    world
        .bind_compute_buffer(id, 0, "_Output".into(), second)
        .unwrap();
    world.dispatch_compute(id, 0, [2, 1, 1]).unwrap();

    let work = dispatched(&mut world);
    let ints_and_buffers: Vec<_> = work
        .iter()
        .map(|record| match record {
            OffscreenWork::Dispatch {
                groups, bindings, ..
            } => (
                groups[0],
                bindings.properties.int("_N"),
                bindings.buffers.clone(),
            ),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        ints_and_buffers,
        vec![
            (1, Some(1), vec![("_Output".to_owned(), first)]),
            (2, Some(2), vec![("_Output".to_owned(), second)]),
        ]
    );
}

#[test]
fn dispatches_and_blits_keep_the_order_they_were_called_in() {
    let mut world = world();
    let id = compute(&mut world);
    let a = render_texture(&mut world);
    let b = render_texture(&mut world);
    let blit = |source, target| OffscreenWork::Blit {
        source,
        target,
        material: None::<MaterialId>,
        pass: -1,
    };
    world.record_offscreen(blit(TextureRef::None, a)).unwrap();
    world.dispatch_compute(id, 1, [1, 1, 1]).unwrap();
    world
        .record_offscreen(blit(TextureRef::Render(a), b))
        .unwrap();
    world.dispatch_compute(id, 0, [3, 3, 3]).unwrap();

    let kinds: Vec<_> = dispatched(&mut world)
        .iter()
        .map(|record| match record {
            OffscreenWork::Blit { .. } => "blit".to_owned(),
            OffscreenWork::Dispatch { kernel, groups, .. } => {
                format!("dispatch {kernel} {}", groups[0])
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(kinds, ["blit", "dispatch 1 1", "blit", "dispatch 0 3"]);
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_dispatch_is_refused_for_a_stale_shader_a_missing_kernel_or_no_groups() {
    let mut world = world();
    let id = compute(&mut world);
    assert_eq!(
        graphics_error(world.dispatch_compute(id, 2, [1, 1, 1])),
        GraphicsError::InvalidKernel
    );
    for groups in [
        [0, 1, 1],
        [1, 0, 1],
        [1, 1, 0],
        [65536, 1, 1],
        [1, 1, 65536],
    ] {
        assert_eq!(
            graphics_error(world.dispatch_compute(id, 0, groups)),
            GraphicsError::InvalidOffscreen,
            "{groups:?}"
        );
    }
    world
        .dispatch_compute(id, 0, [65535, 65535, 65535])
        .unwrap();
    world.destroy_compute_shader(id).unwrap();
    assert_eq!(
        graphics_error(world.dispatch_compute(id, 0, [1, 1, 1])),
        GraphicsError::StaleComputeShader
    );
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn a_buffer_destroyed_after_the_dispatch_leaves_the_record_in_the_list() {
    let mut world = world();
    let id = compute(&mut world);
    let buffer: ComputeBufferId = world.create_compute_buffer(1, 4, 0).unwrap();
    world
        .bind_compute_buffer(id, 0, "_Output".into(), buffer)
        .unwrap();
    world.dispatch_compute(id, 0, [1, 1, 1]).unwrap();
    world.destroy_compute_buffer(buffer).unwrap();
    let work = dispatched(&mut world);
    assert_eq!(work.len(), 1);
    assert!(world.compute_buffer(buffer).is_err());
}

#[test]
fn recording_a_dispatch_directly_checks_the_shader_and_kernel() {
    let mut world = world();
    let id = compute(&mut world);
    let record = |kernel| OffscreenWork::Dispatch {
        shader: id,
        kernel,
        groups: [1, 1, 1],
        bindings: worldrunner_world_simulation::graphics::ComputeBindings::default(),
    };
    world.record_offscreen(record(1)).unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(record(2))),
        GraphicsError::InvalidKernel
    );
    assert_eq!(world.offscreen_work().len(), 1);
}

fn dispatched_heights(world: &mut World) -> Vec<TextureRef> {
    dispatched(world)
        .into_iter()
        .map(|record| match record {
            OffscreenWork::Dispatch { bindings, .. } => {
                bindings.properties.texture("_Heights").unwrap().texture
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

#[test]
fn a_dispatch_resolves_a_global_texture_to_what_the_global_holds_at_that_dispatch() {
    let mut world = world();
    let id = compute(&mut world);
    let target = render_texture(&mut world);
    world
        .set_compute_property(id, Some(0), global_slot("_Terrain"))
        .unwrap();
    world.dispatch_compute(id, 0, [1, 1, 1]).unwrap();
    for texture in [
        TextureRef::Asset(7),
        TextureRef::Render(target),
        TextureRef::None,
    ] {
        world
            .set_global_property(global_texture("_Terrain", texture))
            .unwrap();
        world.dispatch_compute(id, 0, [1, 1, 1]).unwrap();
    }
    assert_eq!(
        dispatched_heights(&mut world),
        [
            TextureRef::None,
            TextureRef::Asset(7),
            TextureRef::Render(target),
            TextureRef::None,
        ],
        "an unset global reads as none, a set one as the asset or render texture it holds"
    );
}

#[test]
fn changing_the_global_after_a_dispatch_does_not_change_the_record() {
    let mut world = world();
    let id = compute(&mut world);
    world
        .set_compute_property(id, Some(0), global_slot("_Terrain"))
        .unwrap();
    world
        .set_global_property(global_texture("_Terrain", TextureRef::Asset(7)))
        .unwrap();
    world.dispatch_compute(id, 0, [1, 1, 1]).unwrap();
    world
        .set_global_property(global_texture("_Terrain", TextureRef::Asset(8)))
        .unwrap();
    world.clear_global_properties();
    assert_eq!(dispatched_heights(&mut world), [TextureRef::Asset(7)]);
}

#[test]
fn a_resolved_texture_keeps_the_scale_and_offset_of_the_kernel_slot() {
    let mut world = world();
    let id = compute(&mut world);
    let slot = TextureSlot {
        texture: TextureRef::Global("_Terrain".into()),
        scale: [2.0, 3.0],
        offset: [0.5, 0.25],
    };
    world
        .set_compute_property(id, Some(0), PropertyValue::Texture("_Heights".into(), slot))
        .unwrap();
    world
        .set_global_property(global_texture("_Terrain", TextureRef::Asset(7)))
        .unwrap();
    world.dispatch_compute(id, 0, [1, 1, 1]).unwrap();
    let OffscreenWork::Dispatch { bindings, .. } = dispatched(&mut world).remove(0) else {
        panic!("expected a dispatch");
    };
    assert_eq!(
        bindings.properties.texture("_Heights"),
        Some(TextureSlot {
            texture: TextureRef::Asset(7),
            scale: [2.0, 3.0],
            offset: [0.5, 0.25],
        })
    );
}

#[test]
fn no_recorded_dispatch_holds_a_global_texture() {
    let mut world = world();
    let id = compute(&mut world);
    let mut bindings = worldrunner_world_simulation::graphics::ComputeBindings::default();
    bindings.properties.textures.push((
        "_Heights".into(),
        TextureSlot {
            texture: TextureRef::Global("_Terrain".into()),
            ..TextureSlot::default()
        },
    ));
    let record = OffscreenWork::Dispatch {
        shader: id,
        kernel: 0,
        groups: [1, 1, 1],
        bindings,
    };
    assert_eq!(
        graphics_error(world.record_offscreen(record)),
        GraphicsError::InvalidTexture
    );
    assert!(world.offscreen_work().is_empty());
}
