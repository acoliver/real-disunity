use worldrunner_world_simulation::{
    InputAction, InputEvent, InputPhase, TextInput, TimeSpan, World, WorldError, WorldSettings,
};
#[test]
fn committed_text_projects_in_order_for_each_phase_and_clears_next_frame() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let text = [
        TextInput::Commit("X".into()),
        TextInput::Commit("X".into()),
        TextInput::Preedit {
            text: "é".into(),
            cursor: Some((2, 2)),
        },
        TextInput::Cancel,
    ];
    let events: Vec<_> = text
        .iter()
        .map(|text| InputEvent {
            timestamp: TimeSpan::ZERO,
            action: InputAction::Text(text.clone()),
        })
        .collect();
    w.push_input(&events).unwrap();
    w.advance(TimeSpan::new(20, 1000).unwrap()).unwrap();
    for phase in [InputPhase::Fixed, InputPhase::Dynamic] {
        assert_eq!(w.input(phase).text, text);
    }
    w.advance(TimeSpan::new(20, 1000).unwrap()).unwrap();
    for phase in [InputPhase::Fixed, InputPhase::Dynamic] {
        assert!(w.input(phase).text.is_empty());
    }
}
#[test]
fn malformed_preedit_rejects_batch_and_unfocused_text_is_not_projected() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let event = |action| InputEvent {
        timestamp: TimeSpan::ZERO,
        action,
    };
    assert_eq!(
        w.push_input(&[
            event(InputAction::Text(TextInput::Commit("X".into()))),
            event(InputAction::Text(TextInput::Preedit {
                text: "é".into(),
                cursor: Some((1, 1))
            }))
        ]),
        Err(WorldError::InvalidInput)
    );
    w.push_input(&[
        event(InputAction::Focus(false)),
        event(InputAction::Text(TextInput::Commit("X".into()))),
    ])
    .unwrap();
    w.advance(TimeSpan::new(20, 1000).unwrap()).unwrap();
    assert_eq!(w.input(InputPhase::Dynamic).text, [TextInput::Cancel]);
}
