#![cfg(feature = "ui")]
//! The canvas group component: defaults, alpha range, one per object, cloning, destruction and
//! the alpha it gives drawn text.
mod ui_font;

use worldrunner_world_simulation::ui::contribution::extract_ui;
use worldrunner_world_simulation::ui::{
    Canvas, CanvasGroup, CanvasGroupFlags, FontBook, RectTransform, TextRun, UiError, UiLimits,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ComponentHandle, DestroyWhen, ObjectDesc, ObjectHandle,
    ParentSpace, World, WorldError, WorldSettings,
};

fn world_with(group: CanvasGroup) -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::CanvasGroup(group))
        .unwrap();
    (world, object)
}

fn handle_of(world: &World, object: ObjectHandle) -> Option<ComponentHandle> {
    world
        .snapshot()
        .components
        .into_iter()
        .find_map(|component| {
            (component.object == object && matches!(component.desc, ComponentDesc::CanvasGroup(_)))
                .then_some(component.handle)
        })
}

fn group_of(world: &World, object: ObjectHandle) -> CanvasGroup {
    let handle = handle_of(world, object).expect("a canvas group on the object");
    match world.component(handle).unwrap().desc {
        ComponentDesc::CanvasGroup(group) => group,
        _ => unreachable!("handle_of finds canvas groups"),
    }
}

fn distinct() -> CanvasGroup {
    CanvasGroup {
        enabled: false,
        alpha: 0.25,
        flags: CanvasGroupFlags {
            interactable: false,
            blocks_raycasts: false,
            ignore_parent_groups: true,
        },
    }
}

#[test]
fn a_new_group_is_enabled_opaque_interactable_and_blocks_raycasts() {
    let group = CanvasGroup::default();
    assert!(group.enabled);
    assert_eq!(group.alpha.to_bits(), 1f32.to_bits());
    assert!(group.flags.interactable);
    assert!(group.flags.blocks_raycasts);
    assert!(!group.flags.ignore_parent_groups);
}

#[test]
fn normalizing_clamps_alpha_and_rejects_nan() {
    for (given, expected) in [
        (-3., 0.),
        (0., 0.),
        (0.5, 0.5),
        (1., 1.),
        (7., 1.),
        (f32::INFINITY, 1.),
        (f32::NEG_INFINITY, 0.),
    ] {
        let group = CanvasGroup {
            alpha: given,
            ..CanvasGroup::default()
        };
        assert_eq!(
            group.normalized().unwrap().alpha.to_bits(),
            f32::to_bits(expected),
            "alpha {given}"
        );
    }
    let nan = CanvasGroup {
        alpha: f32::NAN,
        ..CanvasGroup::default()
    };
    assert_eq!(nan.normalized(), Err(UiError::InvalidCanvasGroup));
}

#[test]
fn the_world_stores_only_alpha_in_range() {
    let (mut world, object) = world_with(CanvasGroup::default());
    let handle = handle_of(&world, object).unwrap();
    for alpha in [-0.5, 1.5, f32::NAN, f32::INFINITY] {
        let bad = CanvasGroup {
            alpha,
            ..CanvasGroup::default()
        };
        assert_eq!(
            world.set_component(handle, ComponentDesc::CanvasGroup(bad)),
            Err(WorldError::Ui(UiError::InvalidCanvasGroup)),
            "alpha {alpha}"
        );
        let other = world.create_object(ObjectDesc::default()).unwrap();
        assert_eq!(
            world.add_component(other, ComponentDesc::CanvasGroup(bad)),
            Err(WorldError::Ui(UiError::InvalidCanvasGroup))
        );
    }
    assert_eq!(group_of(&world, object), CanvasGroup::default());
    for alpha in [0., 1.] {
        let edge = CanvasGroup {
            alpha,
            ..CanvasGroup::default()
        };
        world
            .set_component(handle, ComponentDesc::CanvasGroup(edge))
            .unwrap();
        assert_eq!(
            group_of(&world, object).alpha.to_bits(),
            f32::to_bits(alpha)
        );
    }
}

#[test]
fn every_setting_is_stored() {
    let (mut world, object) = world_with(CanvasGroup::default());
    let handle = handle_of(&world, object).unwrap();
    world
        .set_component(handle, ComponentDesc::CanvasGroup(distinct()))
        .unwrap();
    assert_eq!(group_of(&world, object), distinct());
}

#[test]
fn an_object_has_one_group() {
    let (mut world, object) = world_with(CanvasGroup::default());
    assert_eq!(
        world.add_component(object, ComponentDesc::CanvasGroup(CanvasGroup::default())),
        Err(WorldError::InvalidComponent(ComponentError::Duplicate))
    );
    let handle = handle_of(&world, object).unwrap();
    world.remove_component(handle).unwrap();
    world
        .add_component(object, ComponentDesc::CanvasGroup(CanvasGroup::default()))
        .unwrap();
}

#[test]
fn a_clone_keeps_the_settings_and_owns_them() {
    let (mut world, object) = world_with(distinct());
    let clone = world.instantiate(object, None).unwrap().root;
    assert_eq!(group_of(&world, clone), distinct());
    let handle = handle_of(&world, clone).unwrap();
    world
        .set_component(handle, ComponentDesc::CanvasGroup(CanvasGroup::default()))
        .unwrap();
    assert_eq!(group_of(&world, object), distinct());
}

#[test]
fn destroying_the_object_or_the_component_removes_the_group() {
    let (mut world, object) = world_with(CanvasGroup::default());
    let baseline = world.storage_counts();
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    world.finish_phase();
    assert_eq!(world.storage_counts(), (baseline.0 - 1, baseline.1 - 2));

    let (mut world, object) = world_with(CanvasGroup::default());
    let handle = handle_of(&world, object).unwrap();
    world.remove_component(handle).unwrap();
    assert!(world.component(handle).is_err());
    assert!(handle_of(&world, object).is_none());
}

fn alpha_group(alpha: f32) -> CanvasGroup {
    CanvasGroup {
        alpha,
        ..CanvasGroup::default()
    }
}

/// One text under a chain of rectangles; the first is the canvas root. Each entry is the group on
/// that rectangle, if any. Returns the alpha the text is drawn with.
fn drawn_alpha(chain: &[Option<CanvasGroup>]) -> f32 {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let mut parent = None;
    let mut last = None;
    for group in chain {
        let object = world.create_object(ObjectDesc::default()).unwrap();
        let rect = ComponentDesc::RectTransform(RectTransform::default());
        world.add_component(object, rect).unwrap();
        world
            .set_parent(object, parent, ParentSpace::KeepLocal)
            .unwrap();
        if parent.is_none() {
            let canvas = ComponentDesc::Canvas(Canvas::default());
            world.add_component(object, canvas).unwrap();
        }
        if let Some(group) = group {
            let group = ComponentDesc::CanvasGroup(*group);
            world.add_component(object, group).unwrap();
        }
        parent = Some(object);
        last = Some(object);
    }
    let text = TextRun {
        text: "A".into(),
        color: [1., 1., 1., 0.8],
        ..TextRun::new(font)
    };
    let last = last.unwrap();
    world
        .add_component(last, ComponentDesc::Text(text))
        .unwrap();
    let plan = extract_ui(&world, &fonts, [800, 600]).unwrap();
    assert_eq!(plan.texts.len(), 1);
    plan.texts[0].color[3]
}

#[test]
fn text_without_a_group_keeps_its_own_alpha() {
    assert_eq!(drawn_alpha(&[None, None]).to_bits(), 0.8f32.to_bits());
}

#[test]
fn text_is_drawn_with_the_alpha_of_every_enabled_group_above_it() {
    let alpha = drawn_alpha(&[
        Some(alpha_group(0.5)),
        Some(alpha_group(0.5)),
        Some(alpha_group(0.5)),
    ]);
    assert!((alpha - 0.8 * 0.125).abs() < 1e-6, "alpha {alpha}");
}

#[test]
fn a_group_on_the_text_object_applies_to_the_text() {
    let alpha = drawn_alpha(&[None, Some(alpha_group(0.5))]);
    assert!((alpha - 0.4).abs() < 1e-6, "alpha {alpha}");
}

#[test]
fn a_disabled_group_does_not_change_alpha() {
    let disabled = CanvasGroup {
        enabled: false,
        ..alpha_group(0.)
    };
    let alpha = drawn_alpha(&[Some(alpha_group(0.5)), Some(disabled)]);
    assert!((alpha - 0.4).abs() < 1e-6, "alpha {alpha}");
}

#[test]
fn a_group_that_ignores_parent_groups_stops_the_groups_above_it() {
    let ignoring = CanvasGroup {
        flags: CanvasGroupFlags {
            ignore_parent_groups: true,
            ..CanvasGroupFlags::default()
        },
        ..alpha_group(0.5)
    };
    let alpha = drawn_alpha(&[
        Some(alpha_group(0.1)),
        Some(ignoring),
        Some(alpha_group(0.5)),
    ]);
    assert!((alpha - 0.2).abs() < 1e-6, "alpha {alpha}");
}
