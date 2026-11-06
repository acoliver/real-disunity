use super::*;
use crate::retained::ui_font;
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{ComponentDesc, ParentSpace, TextInput};

fn field_plan() -> UiRenderPlan {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    world
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(object, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform {
                size_delta: glam::Vec2::new(32., 16.),
                ..Default::default()
            }),
        )
        .unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let mut run = TextRun::new(fonts.register(ui_font::font()).unwrap());
    run.font_size = 10.;
    run.color = [0.; 4];
    let mut router = UiRouter::default();
    router.controls.insert(object, UiControl::text_field());
    router
        .text_fields
        .insert(object, UiTextField::new(run, 10, &fonts).unwrap());
    let layout = world.ui_layout([64, 64].into()).unwrap();
    router
        .key(&world, &layout, UiKey::Tab, true, false)
        .unwrap();
    router
        .text(&world, &layout, &fonts, &TextInput::Commit("AB".into()))
        .unwrap();
    router
        .key(&world, &layout, UiKey::SelectAll, true, false)
        .unwrap();
    let mut plan = contribution::extract_ui_layout(&world, &fonts, layout).unwrap();
    router
        .text_fields_plan(
            &world,
            &fonts,
            &mut plan,
            TextFieldStyle {
                caret: [1., 0., 0., 1.],
                selection: [0., 1., 0., 1.],
            },
        )
        .unwrap();
    assert_eq!(plan.graphics[0].graphic.quads.len(), 2);
    plan
}
#[test]
fn windowless_text_field_draws_caret_selection_and_delivers_scale_changes() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    apply(&mut run, field_plan());
    let image = capture(&mut run, &events).1;
    assert_eq!(pixel(&image, 28, 25), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 18, 25), [0, 255, 0, 255]);
    run.set_display_scale(2.).unwrap();
    assert_eq!(run.host().scale_factor().to_bits(), 2_f64.to_bits());
    assert_eq!(run.host().size(), [64, 64]);
    run.resize([128, 128]).unwrap();
    assert!(events.borrow().contains(&RetainedEvent::Input(
        crate::RetainedInput::DisplayChanged {
            scale_factor: 2.,
            physical_size: [128, 128]
        }
    )));
    assert!(run.set_display_scale(0.).is_err());
    run.shutdown().unwrap();
}

#[test]
fn text_field_and_annotations_share_overlay_contributions_without_a_gpu() {
    use worldrunner_camera::{CameraLens, CameraPose, CameraState, ViewConvention, Viewport};
    use worldrunner_world_simulation::ui::annotations::*;
    use worldrunner_world_simulation::ui::presentation::{UiDrawKind, UiPresentation};

    let plan = field_plan();
    assert!(plan.graphics[0].graphic.triangles.is_empty());
    let mut presentation = UiPresentation::from_plan(&plan);
    let field_draws = presentation.draws.clone();
    assert_eq!(field_draws.len(), 4);
    assert_eq!(presentation.coverage.len(), 1);
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let mut run = TextRun::new(fonts.register(ui_font::font()).unwrap());
    run.text = "AB".into();
    let view = worldrunner_camera::resolve(&CameraState {
        pose: CameraPose {
            eye_meters: glam::DVec3::Z * 5.,
            forward: -glam::DVec3::Z,
            up: glam::DVec3::Y,
        },
        lens: CameraLens::Perspective {
            vertical_fov_radians: 1.,
            near_meters: 0.1,
            far_meters: 100.,
        },
        viewport: Viewport {
            width: 64,
            height: 64,
        },
        convention: ViewConvention::RIGHT_HANDED,
    })
    .unwrap();
    let annotation = Annotation {
        primitive: AnnotationPrimitive::Arrow {
            from: AnnotationPoint::Screen(glam::Vec2::new(8., 16.)),
            to: AnnotationPoint::Screen(glam::Vec2::new(40., 16.)),
            width: 4.,
            head_length: 12.,
            head_width: 16.,
            colour: [1., 0., 0., 1.],
        },
        draw_order: (1, 0, 0),
        clip_rect: None,
        depth: AnnotationDepth::AlwaysVisible,
    };
    let label = Annotation {
        primitive: AnnotationPrimitive::Label {
            anchor: AnnotationPoint::Screen(glam::Vec2::splat(32.)),
            run,
            alignment: glam::Vec2::ZERO,
        },
        ..annotation.clone()
    };
    presentation
        .append(extract_annotations(&[annotation, label], &fonts, &view).unwrap())
        .unwrap();
    assert_eq!(&presentation.draws[..field_draws.len()], &field_draws);
    assert_eq!(presentation.coverage.len(), 2);
    assert_eq!(presentation.draws.len(), 9);
    assert_eq!(
        presentation
            .draws
            .iter()
            .filter(|d| d.triangle_uv.is_some())
            .count(),
        3
    );
    let text_ids: std::collections::BTreeSet<_> = presentation
        .draws
        .iter()
        .filter(|d| d.kind == UiDrawKind::Text)
        .map(|d| d.texture.unwrap())
        .collect();
    assert_eq!(text_ids, [0, 1].into());
    assert!(
        UiOverlay::from_presentation(&presentation, &std::collections::BTreeMap::new()).is_ok()
    );
}
