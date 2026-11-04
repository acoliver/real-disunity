use super::*;

#[test]
fn captured_slider_clamps_extreme_finite_host_positions() {
    let (w, mut r, _, object) = fixture(slider(None));
    moved(&w, &mut r, 200., 150.);
    button(&w, &mut r, true);
    assert_eq!(
        moved(&w, &mut r, f64::MAX, f64::MAX).actions,
        changed(object, 10.)
    );
    assert_eq!(
        moved(&w, &mut r, -f64::MAX, -f64::MAX).actions,
        changed(object, -10.)
    );
    assert!(r.slider(object).unwrap().value().is_finite());
}
#[test]
fn extreme_diagonal_positions_do_not_publish_nan_on_rotated_tracks() {
    let (mut w, mut r, _, object) = fixture(slider(None));
    w.set_local_transform(
        object,
        worldrunner_world_simulation::LocalTransform {
            rotation: glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
            ..Default::default()
        },
    )
    .unwrap();
    moved(&w, &mut r, 200., 150.);
    button(&w, &mut r, true);
    moved(&w, &mut r, f64::MAX, f64::MAX);
    assert!(r.slider(object).unwrap().value().is_finite());
}
#[test]
fn track_ends_are_exact_even_for_asymmetric_floating_point_ranges() {
    let control =
        UiControl::slider(UiSlider::new(-1., 1e-20, None, -1., UiAxis::Horizontal).unwrap());
    let (w, mut r, _, object) = fixture(control);
    moved(&w, &mut r, 250., 150.);
    assert_eq!(button(&w, &mut r, true).actions, changed(object, 1e-20));
    assert_eq!(
        r.slider(object).unwrap().value().to_bits(),
        1e-20_f64.to_bits()
    );
}
#[test]
fn invalid_pointer_event_does_not_mutate_captured_value() {
    let (w, mut r, _, object) = fixture(slider(None));
    moved(&w, &mut r, 200., 150.);
    button(&w, &mut r, true);
    assert!(
        r.pointer(&w, &layout(&w), PointerEvent::Moved([f64::NAN, 150.]))
            .is_err()
    );
    assert_eq!(r.slider(object).unwrap().value().to_bits(), 0_f64.to_bits());
    assert_eq!(moved(&w, &mut r, 225., 150.).actions, changed(object, 5.));
}
#[test]
fn new_modal_or_hidden_track_cancels_slider_capture() {
    let (mut w, mut r, root, object) = fixture(slider(None));
    let modal = rect(&mut w, Some(root), Vec2::new(0., 50.));
    moved(&w, &mut r, 200., 150.);
    button(&w, &mut r, true);
    r.modals.insert(modal);
    assert!(moved(&w, &mut r, 225., 150.).actions.is_empty());
    r.modals.clear();
    assert!(moved(&w, &mut r, 240., 150.).actions.is_empty());
    button(&w, &mut r, false);
    button(&w, &mut r, true);
    w.set_active(object, false).unwrap();
    assert!(moved(&w, &mut r, 220., 150.).actions.is_empty());
    w.set_active(object, true).unwrap();
    assert!(moved(&w, &mut r, 210., 150.).actions.is_empty());
}
fn add_graphic(w: &mut World, object: ObjectHandle) {
    let vertices = [
        [-50., -10., 0.],
        [50., -10., 0.],
        [50., 10., 0.],
        [-50., 10., 0.],
    ]
    .map(|position| UiVertex {
        position,
        uv: [0.; 2],
        color: [1.; 4],
    });
    w.add_component(
        object,
        ComponentDesc::CanvasRenderer(CanvasRenderer {
            geometry: Some(std::sync::Arc::new(
                UiGeometry::new(vertices.to_vec(), vec![0, 1, 2, 0, 2, 3]).unwrap(),
            )),
            ..Default::default()
        }),
    )
    .unwrap();
}
fn tint(w: &World, r: &UiRouter) -> [u32; 4] {
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let mut plan = contribution::extract_ui_layout(w, &fonts, layout(w)).unwrap();
    r.style_plan(w, &mut plan).unwrap();
    plan.graphics[0].graphic.quads[0].color.map(f32::to_bits)
}
#[test]
fn value_controls_use_existing_style_states_and_toggle_selection() {
    for control in [slider(None), UiControl::toggle(false)] {
        let (mut w, mut r, _, object) = fixture(control);
        add_graphic(&mut w, object);
        let style = UiStyle {
            normal: [0.1; 4],
            selected: [0.2; 4],
            hovered: [0.3; 4],
            focused: [0.4; 4],
            pressed: [0.5; 4],
            disabled: [0.6; 4],
        };
        r.styles.insert(object, style);
        assert_eq!(tint(&w, &r), style.normal.map(f32::to_bits));
        moved(&w, &mut r, 200., 150.);
        assert_eq!(tint(&w, &r), style.hovered.map(f32::to_bits));
        button(&w, &mut r, true);
        assert_eq!(tint(&w, &r), style.pressed.map(f32::to_bits));
        button(&w, &mut r, false);
        assert_eq!(tint(&w, &r), style.focused.map(f32::to_bits));
        pointer(&w, &mut r, PointerEvent::Cancel);
        let expected = match control.kind {
            UiControlKind::Toggle { .. } => style.selected,
            _ => style.normal,
        };
        assert_eq!(tint(&w, &r), expected.map(f32::to_bits));
        r.controls.get_mut(&object).unwrap().enabled = false;
        assert_eq!(tint(&w, &r), style.disabled.map(f32::to_bits));
    }
}
#[test]
fn slider_query_supplies_fill_fraction_and_orientation() {
    let control =
        UiControl::slider(UiSlider::new(10., 30., Some(2.), 17., UiAxis::Vertical).unwrap());
    let (_, r, _, object) = fixture(control);
    let state = r.slider(object).unwrap();
    assert_eq!(state.value().to_bits(), 18_f64.to_bits());
    assert_eq!(state.normalized_value().to_bits(), 0.4_f64.to_bits());
    assert_eq!(state.axis(), UiAxis::Vertical);
}
