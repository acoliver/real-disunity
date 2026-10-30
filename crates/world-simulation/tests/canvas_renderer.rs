#![cfg(feature = "ui")]
//! The canvas renderer component: defaults, alpha, clip rectangle, material limits, one per
//! object, cloning, destruction and the alpha it inherits from canvas groups.
use worldrunner_world_simulation::ui::{
    CanvasGroup, CanvasGroupFlags, CanvasRenderer, MAX_CANVAS_MATERIALS, UiError, inherited_alpha,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ComponentHandle, DestroyWhen, ObjectDesc, ObjectHandle,
    ParentSpace, World, WorldError, WorldSettings,
};

fn world_with(renderer: CanvasRenderer) -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::CanvasRenderer(renderer))
        .unwrap();
    (world, object)
}

fn handle_of(world: &World, object: ObjectHandle) -> Option<ComponentHandle> {
    world
        .snapshot()
        .components
        .into_iter()
        .find_map(|component| {
            (component.object == object
                && matches!(component.desc, ComponentDesc::CanvasRenderer(_)))
            .then_some(component.handle)
        })
}

fn renderer_of(world: &World, object: ObjectHandle) -> CanvasRenderer {
    let handle = handle_of(world, object).expect("a canvas renderer on the object");
    match world.component(handle).unwrap().desc {
        ComponentDesc::CanvasRenderer(renderer) => renderer,
        _ => unreachable!("handle_of finds canvas renderers"),
    }
}

fn distinct() -> CanvasRenderer {
    let mut renderer = CanvasRenderer {
        color: [0.1, 0.2, 0.3, 0.4],
        alpha: 0.5,
        clip_rect: Some([1., 2., 3., 4.]),
        clipping_softness: [5., 6.],
        material_count: 2,
        pop_material_count: 1,
        mesh: 11,
        texture: 12,
        alpha_texture: 13,
        ..CanvasRenderer::default()
    };
    renderer.flags.cull = true;
    renderer.flags.cull_transparent_mesh = true;
    renderer.flags.has_pop_instruction = true;
    renderer.materials[..2].copy_from_slice(&[21, 22]);
    renderer.pop_materials[0] = 31;
    renderer
}

#[test]
fn a_new_renderer_is_white_opaque_unclipped_and_empty() {
    let renderer = CanvasRenderer::default();
    assert_eq!(renderer.color.map(f32::to_bits), [1f32.to_bits(); 4]);
    assert_eq!(renderer.alpha.to_bits(), 1f32.to_bits());
    assert!(!renderer.flags.cull);
    assert!(!renderer.flags.cull_transparent_mesh);
    assert!(!renderer.flags.has_pop_instruction);
    assert_eq!(renderer.clip_rect, None);
    assert_eq!(
        renderer.clipping_softness.map(f32::to_bits),
        [0f32.to_bits(); 2]
    );
    assert_eq!(
        (renderer.material_count, renderer.pop_material_count),
        (0, 0)
    );
    assert_eq!(
        (renderer.mesh, renderer.texture, renderer.alpha_texture),
        (0, 0, 0)
    );
    assert_eq!(renderer.materials, [0; MAX_CANVAS_MATERIALS]);
}

#[test]
fn every_setting_is_stored() {
    let (mut world, object) = world_with(CanvasRenderer::default());
    let handle = handle_of(&world, object).unwrap();
    world
        .set_component(handle, ComponentDesc::CanvasRenderer(distinct()))
        .unwrap();
    assert_eq!(renderer_of(&world, object), distinct());
}

#[test]
fn a_nan_alpha_is_rejected_and_any_other_alpha_is_kept() {
    let (mut world, object) = world_with(CanvasRenderer::default());
    let handle = handle_of(&world, object).unwrap();
    let nan = CanvasRenderer {
        alpha: f32::NAN,
        ..CanvasRenderer::default()
    };
    assert_eq!(
        world.set_component(handle, ComponentDesc::CanvasRenderer(nan)),
        Err(WorldError::Ui(UiError::InvalidCanvasRenderer))
    );
    let other = world.create_object(ObjectDesc::default()).unwrap();
    assert_eq!(
        world.add_component(other, ComponentDesc::CanvasRenderer(nan)),
        Err(WorldError::Ui(UiError::InvalidCanvasRenderer))
    );
    assert_eq!(nan.normalized(), Err(UiError::InvalidCanvasRenderer));
    assert_eq!(renderer_of(&world, object), CanvasRenderer::default());
    for alpha in [-2., 0., 0.3, 7., f32::INFINITY] {
        let given = CanvasRenderer {
            alpha,
            ..CanvasRenderer::default()
        };
        world
            .set_component(handle, ComponentDesc::CanvasRenderer(given))
            .unwrap();
        assert_eq!(renderer_of(&world, object).alpha.to_bits(), alpha.to_bits());
    }
}

#[test]
fn a_clip_rectangle_is_stored_and_can_be_removed() {
    let (mut world, object) = world_with(CanvasRenderer::default());
    let handle = handle_of(&world, object).unwrap();
    let clipped = CanvasRenderer {
        clip_rect: Some([-1., 2., 30., 40.]),
        clipping_softness: [0.5, 1.5],
        ..CanvasRenderer::default()
    };
    world
        .set_component(handle, ComponentDesc::CanvasRenderer(clipped))
        .unwrap();
    assert_eq!(
        renderer_of(&world, object).clip_rect,
        Some([-1., 2., 30., 40.])
    );
    assert_eq!(
        renderer_of(&world, object)
            .clipping_softness
            .map(f32::to_bits),
        [0.5f32.to_bits(), 1.5f32.to_bits()]
    );
    let unclipped = CanvasRenderer {
        clip_rect: None,
        ..clipped
    };
    world
        .set_component(handle, ComponentDesc::CanvasRenderer(unclipped))
        .unwrap();
    assert_eq!(renderer_of(&world, object).clip_rect, None);
}

#[test]
fn material_counts_are_limited_to_the_slots() {
    let (mut world, object) = world_with(CanvasRenderer::default());
    let handle = handle_of(&world, object).unwrap();
    let limit = u32::try_from(MAX_CANVAS_MATERIALS).unwrap();
    let full = CanvasRenderer {
        material_count: limit,
        pop_material_count: limit,
        ..CanvasRenderer::default()
    };
    world
        .set_component(handle, ComponentDesc::CanvasRenderer(full))
        .unwrap();
    assert_eq!(renderer_of(&world, object).material_count, limit);
    for (materials, pops) in [(limit + 1, 0), (0, limit + 1)] {
        let over = CanvasRenderer {
            material_count: materials,
            pop_material_count: pops,
            ..CanvasRenderer::default()
        };
        assert_eq!(
            world.set_component(handle, ComponentDesc::CanvasRenderer(over)),
            Err(WorldError::Ui(UiError::InvalidCanvasRenderer)),
            "counts {materials} and {pops}"
        );
        assert_eq!(over.normalized(), Err(UiError::InvalidCanvasRenderer));
    }
    assert_eq!(renderer_of(&world, object).material_count, limit);
}

#[test]
fn slots_past_the_count_are_cleared() {
    let mut renderer = distinct();
    renderer.materials[5] = 99;
    renderer.pop_materials[6] = 98;
    let normal = renderer.normalized().unwrap();
    assert_eq!(normal.materials, distinct().materials);
    assert_eq!(normal.pop_materials, distinct().pop_materials);
}

#[test]
fn a_material_is_set_only_inside_the_count() {
    let renderer = CanvasRenderer {
        material_count: 2,
        pop_material_count: 1,
        ..CanvasRenderer::default()
    };
    let set = renderer.with_material(false, 1, 7).unwrap();
    assert_eq!(set.materials[1], 7);
    assert_eq!(set.pop_materials, [0; MAX_CANVAS_MATERIALS]);
    let pop = renderer.with_material(true, 0, 8).unwrap();
    assert_eq!(pop.pop_materials[0], 8);
    assert_eq!(pop.materials, [0; MAX_CANVAS_MATERIALS]);
    assert_eq!(
        renderer.with_material(false, 2, 7),
        Err(UiError::InvalidCanvasRenderer)
    );
    assert_eq!(
        renderer.with_material(true, 1, 7),
        Err(UiError::InvalidCanvasRenderer)
    );
}

#[test]
fn an_object_has_one_renderer() {
    let (mut world, object) = world_with(CanvasRenderer::default());
    assert_eq!(
        world.add_component(
            object,
            ComponentDesc::CanvasRenderer(CanvasRenderer::default())
        ),
        Err(WorldError::InvalidComponent(ComponentError::Duplicate))
    );
    let handle = handle_of(&world, object).unwrap();
    world.remove_component(handle).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::CanvasRenderer(CanvasRenderer::default()),
        )
        .unwrap();
}

#[test]
fn a_clone_keeps_the_settings_and_owns_them() {
    let (mut world, object) = world_with(distinct());
    let clone = world.instantiate(object, None).unwrap().root;
    assert_eq!(renderer_of(&world, clone), distinct());
    let handle = handle_of(&world, clone).unwrap();
    world
        .set_component(
            handle,
            ComponentDesc::CanvasRenderer(CanvasRenderer::default()),
        )
        .unwrap();
    assert_eq!(renderer_of(&world, object), distinct());
}

#[test]
fn destroying_the_object_or_the_component_removes_the_renderer() {
    let (mut world, object) = world_with(CanvasRenderer::default());
    let baseline = world.storage_counts();
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    world.finish_phase();
    assert_eq!(world.storage_counts(), (baseline.0 - 1, baseline.1 - 2));

    let (mut world, object) = world_with(CanvasRenderer::default());
    let handle = handle_of(&world, object).unwrap();
    world.remove_component(handle).unwrap();
    assert!(world.component(handle).is_err());
    assert!(handle_of(&world, object).is_none());
}

fn group(alpha: f32) -> CanvasGroup {
    CanvasGroup {
        alpha,
        ..CanvasGroup::default()
    }
}

/// A chain of objects, root first, each with an optional group; the renderer sits on the last
/// object with `own` as its alpha. Returns the renderer's inherited alpha.
fn inherited(chain: &[Option<CanvasGroup>], own: f32) -> f32 {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut parent = None;
    let mut last = None;
    for entry in chain {
        let object = world.create_object(ObjectDesc::default()).unwrap();
        world
            .set_parent(object, parent, ParentSpace::KeepLocal)
            .unwrap();
        if let Some(entry) = entry {
            let desc = ComponentDesc::CanvasGroup(*entry);
            world.add_component(object, desc).unwrap();
        }
        parent = Some(object);
        last = Some(object);
    }
    let renderer = CanvasRenderer {
        alpha: own,
        ..CanvasRenderer::default()
    };
    let last = last.unwrap();
    world
        .add_component(last, ComponentDesc::CanvasRenderer(renderer))
        .unwrap();
    inherited_alpha(&world, handle_of(&world, last).unwrap()).unwrap()
}

#[test]
fn without_groups_the_inherited_alpha_is_its_own() {
    assert_eq!(inherited(&[None, None], 0.8).to_bits(), 0.8f32.to_bits());
}

#[test]
fn the_inherited_alpha_multiplies_every_enabled_group_above() {
    let alpha = inherited(&[Some(group(0.5)), Some(group(0.5)), Some(group(0.5))], 0.8);
    assert!((alpha - 0.1).abs() < 1e-6, "alpha {alpha}");
}

#[test]
fn a_disabled_group_does_not_change_the_inherited_alpha() {
    let disabled = CanvasGroup {
        enabled: false,
        ..group(0.)
    };
    let alpha = inherited(&[Some(group(0.5)), Some(disabled)], 0.8);
    assert!((alpha - 0.4).abs() < 1e-6, "alpha {alpha}");
}

#[test]
fn a_group_that_ignores_parents_stops_the_walk() {
    let ignoring = CanvasGroup {
        flags: CanvasGroupFlags {
            ignore_parent_groups: true,
            ..CanvasGroupFlags::default()
        },
        ..group(0.5)
    };
    let alpha = inherited(&[Some(group(0.1)), Some(ignoring), None], 0.8);
    assert!((alpha - 0.4).abs() < 1e-6, "alpha {alpha}");
}
