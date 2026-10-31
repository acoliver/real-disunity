//! The mesh renderer and mesh filter components: defaults, validation, one per object, cloning
//! and destruction.
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ComponentHandle, DestroyWhen, MAX_RENDERER_MATERIALS,
    MeshFilter, MeshRenderer, ObjectDesc, ObjectHandle, RendererKind, World, WorldError,
    WorldSettings,
};

fn world_with(desc: ComponentDesc) -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world.add_component(object, desc).unwrap();
    (world, object)
}

fn find(world: &World, object: ObjectHandle, filter: bool) -> Option<ComponentHandle> {
    world
        .snapshot()
        .components
        .into_iter()
        .find_map(|component| {
            let matches = match component.desc {
                ComponentDesc::MeshFilter(_) => filter,
                ComponentDesc::MeshRenderer(_) => !filter,
                _ => false,
            };
            (component.object == object && matches).then_some(component.handle)
        })
}

fn renderer_of(world: &World, object: ObjectHandle) -> MeshRenderer {
    let handle = find(world, object, false).expect("a mesh renderer on the object");
    match world.component(handle).unwrap().desc {
        ComponentDesc::MeshRenderer(renderer) => renderer,
        _ => unreachable!("find returns mesh renderers"),
    }
}

fn filter_of(world: &World, object: ObjectHandle) -> MeshFilter {
    let handle = find(world, object, true).expect("a mesh filter on the object");
    match world.component(handle).unwrap().desc {
        ComponentDesc::MeshFilter(filter) => filter,
        _ => unreachable!("find returns mesh filters"),
    }
}

fn distinct() -> MeshRenderer {
    let mut renderer = MeshRenderer::of_kind(RendererKind::Skinned);
    renderer.switches.enabled = false;
    renderer.switches.force_rendering_off = true;
    renderer.switches.receive_shadows = false;
    renderer.allow_occlusion_when_dynamic = false;
    renderer.static_shadow_caster = true;
    renderer.shadow_casting_mode = 3;
    renderer.motion_vector_mode = 2;
    renderer.light_probe_usage = 4;
    renderer.reflection_probe_usage = 3;
    renderer.rendering_layer_mask = 0xF0F0;
    renderer.renderer_priority = -7;
    renderer.sorting_layer = 0;
    renderer.sorting_order = 42;
    renderer.material_count = 3;
    renderer
}

#[test]
fn a_new_renderer_is_enabled_shadowed_and_has_no_material() {
    let renderer = MeshRenderer::default();
    assert_eq!(renderer.kind, RendererKind::Mesh);
    assert!(renderer.switches.enabled);
    assert!(!renderer.switches.force_rendering_off);
    assert!(renderer.switches.receive_shadows);
    assert!(renderer.allow_occlusion_when_dynamic);
    assert!(!renderer.static_shadow_caster);
    assert_eq!(
        (renderer.shadow_casting_mode, renderer.motion_vector_mode),
        (1, 1)
    );
    assert_eq!(
        (renderer.light_probe_usage, renderer.reflection_probe_usage),
        (1, 1)
    );
    assert_eq!(renderer.rendering_layer_mask, 1);
    assert_eq!(renderer.material_count, 0);
    assert_eq!(
        MeshRenderer::of_kind(RendererKind::Skinned).kind,
        RendererKind::Skinned
    );
    assert_eq!(
        MeshRenderer::of_kind(RendererKind::Particle).kind,
        RendererKind::Particle
    );
    assert_eq!(MeshFilter::default().mesh, 0);
}

#[test]
fn every_setting_is_stored() {
    let (mut world, object) = world_with(ComponentDesc::MeshRenderer(MeshRenderer::default()));
    let handle = find(&world, object, false).unwrap();
    world
        .set_component(handle, ComponentDesc::MeshRenderer(distinct()))
        .unwrap();
    assert_eq!(renderer_of(&world, object), distinct());
}

#[test]
fn a_mesh_identity_is_stored() {
    let (mut world, object) = world_with(ComponentDesc::MeshFilter(MeshFilter::default()));
    let handle = find(&world, object, true).unwrap();
    world
        .set_component(handle, ComponentDesc::MeshFilter(MeshFilter { mesh: 99 }))
        .unwrap();
    assert_eq!(filter_of(&world, object), MeshFilter { mesh: 99 });
}

#[test]
fn modes_and_counts_out_of_range_are_rejected() {
    let (mut world, object) = world_with(ComponentDesc::MeshRenderer(MeshRenderer::default()));
    let handle = find(&world, object, false).unwrap();
    let bad: [fn(&mut MeshRenderer); 5] = [
        |r| r.shadow_casting_mode = 4,
        |r| r.motion_vector_mode = 3,
        |r| r.light_probe_usage = 3,
        |r| r.reflection_probe_usage = 4,
        |r| r.material_count = MAX_RENDERER_MATERIALS + 1,
    ];
    for change in bad {
        let mut renderer = MeshRenderer::default();
        change(&mut renderer);
        let rejected = Err(WorldError::InvalidComponent(
            ComponentError::InvalidMeshRenderer,
        ));
        assert_eq!(
            world.set_component(handle, ComponentDesc::MeshRenderer(renderer)),
            rejected
        );
    }
    assert_eq!(renderer_of(&world, object), MeshRenderer::default());
    let ok = MeshRenderer {
        material_count: MAX_RENDERER_MATERIALS,
        light_probe_usage: 2,
        ..MeshRenderer::default()
    };
    world
        .set_component(handle, ComponentDesc::MeshRenderer(ok))
        .unwrap();
}

#[test]
fn an_object_has_one_of_each() {
    let (mut world, object) = world_with(ComponentDesc::MeshRenderer(MeshRenderer::default()));
    world
        .add_component(object, ComponentDesc::MeshFilter(MeshFilter::default()))
        .unwrap();
    for desc in [
        ComponentDesc::MeshRenderer(MeshRenderer::of_kind(RendererKind::Skinned)),
        ComponentDesc::MeshFilter(MeshFilter { mesh: 1 }),
    ] {
        assert_eq!(
            world.add_component(object, desc),
            Err(WorldError::InvalidComponent(ComponentError::Duplicate))
        );
    }
    let handle = find(&world, object, false).unwrap();
    world.remove_component(handle).unwrap();
    world
        .add_component(object, ComponentDesc::MeshRenderer(MeshRenderer::default()))
        .unwrap();
}

#[test]
fn the_kind_of_a_renderer_cannot_be_changed() {
    let (mut world, object) = world_with(ComponentDesc::MeshRenderer(MeshRenderer::default()));
    let handle = find(&world, object, false).unwrap();
    world
        .set_component(handle, ComponentDesc::MeshFilter(MeshFilter::default()))
        .unwrap_err();
    assert_eq!(renderer_of(&world, object).kind, RendererKind::Mesh);
}

#[test]
fn a_clone_keeps_the_settings_and_owns_them() {
    let (mut world, object) = world_with(ComponentDesc::MeshRenderer(distinct()));
    world
        .add_component(object, ComponentDesc::MeshFilter(MeshFilter { mesh: 5 }))
        .unwrap();
    let clone = world.instantiate(object, None).unwrap().root;
    assert_eq!(renderer_of(&world, clone), distinct());
    assert_eq!(filter_of(&world, clone), MeshFilter { mesh: 5 });
    let handle = find(&world, clone, false).unwrap();
    world
        .set_component(handle, ComponentDesc::MeshRenderer(MeshRenderer::default()))
        .unwrap();
    assert_eq!(renderer_of(&world, object), distinct());
}

#[test]
fn destroying_the_object_or_the_component_removes_them() {
    let (mut world, object) = world_with(ComponentDesc::MeshRenderer(MeshRenderer::default()));
    world
        .add_component(object, ComponentDesc::MeshFilter(MeshFilter::default()))
        .unwrap();
    let baseline = world.storage_counts();
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    world.finish_phase();
    assert_eq!(world.storage_counts(), (baseline.0 - 1, baseline.1 - 3));

    let (mut world, object) = world_with(ComponentDesc::MeshFilter(MeshFilter::default()));
    let handle = find(&world, object, true).unwrap();
    world.remove_component(handle).unwrap();
    assert!(world.component(handle).is_err());
    assert!(find(&world, object, true).is_none());
}
