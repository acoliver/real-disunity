use worldrunner_world_simulation::{
    InputAction, InputEvent, InputPhase, PointerButton, PointerEvent, PointerWheel, TimeSpan,
    World, WorldError, WorldSettings,
};
fn span(n: u64) -> TimeSpan {
    TimeSpan::new(n, 1000).unwrap()
}
fn push(w: &mut World, n: u64, action: InputAction) {
    w.push_input(&[InputEvent {
        timestamp: span(n),
        action,
    }])
    .unwrap();
}
fn pointer(w: &mut World, n: u64, event: PointerEvent) {
    push(w, n, InputAction::Pointer(event));
}
#[test]
fn frame_state_preserves_order_edges_units_and_independent_phases() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    pointer(&mut w, 1, PointerEvent::Moved([100., 80.]));
    pointer(
        &mut w,
        2,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed: true,
        },
    );
    pointer(
        &mut w,
        3,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed: false,
        },
    );
    pointer(
        &mut w,
        4,
        PointerEvent::Wheel(PointerWheel::Lines([1., 2.])),
    );
    pointer(
        &mut w,
        5,
        PointerEvent::Wheel(PointerWheel::Pixels([3., 4.])),
    );
    pointer(
        &mut w,
        6,
        PointerEvent::Wheel(PointerWheel::Lines([-0.5, 1.])),
    );
    w.advance(span(20)).unwrap();
    for phase in [InputPhase::Fixed, InputPhase::Dynamic] {
        let p = &w.input(phase).pointer;
        assert_eq!(p.position, Some([100., 80.]));
        assert!(p.held.is_empty());
        assert!(p.down.contains(&PointerButton::Left));
        assert!(p.up.contains(&PointerButton::Left));
        assert_eq!(p.wheel_lines.map(f64::to_bits), [0.5, 3.].map(f64::to_bits));
        assert_eq!(p.wheel_pixels.map(f64::to_bits), [3., 4.].map(f64::to_bits));
        assert_eq!(p.events.len(), 6);
    }
    w.advance(span(20)).unwrap();
    for phase in [InputPhase::Fixed, InputPhase::Dynamic] {
        let p = &w.input(phase).pointer;
        assert_eq!(p.position, Some([100., 80.]));
        assert!(p.down.is_empty() && p.up.is_empty() && p.events.is_empty());
        assert_eq!(p.wheel_lines.map(f64::to_bits), [0_f64.to_bits(); 2]);
        assert_eq!(p.wheel_pixels.map(f64::to_bits), [0_f64.to_bits(); 2]);
    }
}
#[test]
fn focus_loss_cancels_and_ignores_input_until_focus_returns() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    pointer(
        &mut w,
        1,
        PointerEvent::Button {
            button: PointerButton::Right,
            pressed: true,
        },
    );
    w.advance(span(20)).unwrap();
    assert!(
        w.input(InputPhase::Dynamic)
            .pointer
            .held
            .contains(&PointerButton::Right)
    );
    push(&mut w, 21, InputAction::Focus(false));
    pointer(
        &mut w,
        22,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed: true,
        },
    );
    pointer(&mut w, 23, PointerEvent::Moved([3., 4.]));
    w.advance(span(20)).unwrap();
    let p = &w.input(InputPhase::Dynamic).pointer;
    assert_eq!(p.events, vec![PointerEvent::Cancel]);
    assert!(p.held.is_empty() && p.up.contains(&PointerButton::Right));
    assert_eq!(p.position, None);
    push(&mut w, 41, InputAction::Focus(true));
    pointer(&mut w, 42, PointerEvent::Moved([5., 6.]));
    w.advance(span(20)).unwrap();
    assert_eq!(
        w.input(InputPhase::Dynamic).pointer.position,
        Some([5., 6.])
    );
}
#[test]
fn nonfinite_pointer_observations_reject_the_whole_batch() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    for event in [
        PointerEvent::Moved([f64::NAN, 0.]),
        PointerEvent::Wheel(PointerWheel::Lines([0., f64::INFINITY])),
        PointerEvent::Wheel(PointerWheel::Pixels([f64::NEG_INFINITY, 0.])),
    ] {
        assert_eq!(
            w.push_input(&[
                InputEvent {
                    timestamp: span(1),
                    action: InputAction::Pointer(PointerEvent::Moved([1., 2.]))
                },
                InputEvent {
                    timestamp: span(2),
                    action: InputAction::Pointer(event)
                }
            ]),
            Err(WorldError::InvalidInput)
        );
    }
    w.advance(span(20)).unwrap();
    assert!(w.input(InputPhase::Dynamic).pointer.events.is_empty());
}
