use super::reference::{MESSAGES, assert_phases_match};
use crate::{
    ComponentDesc, ComponentHandle, DestroyWhen, LifecycleWork, ObjectDesc, ObjectHandle,
    ScriptMessage, ScriptMessages, World, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn desc(type_name: &str) -> ComponentDesc {
    ComponentDesc::Script {
        assembly: "Example".into(),
        type_name: type_name.into(),
    }
}

/// A new object with one script, made ready the way the scheduler would.
fn ready_script(world: &mut World, type_name: &str) -> (ObjectHandle, ComponentHandle) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let script = world.add_component(object, desc(type_name)).unwrap();
    world.set_script_enabled(script, true).unwrap();
    (object, script)
}

fn start_all(world: &mut World) {
    for task in world.phase_tasks(ScriptMessage::Start) {
        if let crate::timeline::Task::Work(LifecycleWork { script, .. }) = task {
            world.lifecycle.mark_started(script);
        }
    }
}

fn queued_scripts(
    world: &mut World,
    message: ScriptMessage,
) -> Vec<(ComponentHandle, ScriptMessage)> {
    world
        .phase_tasks(message)
        .into_iter()
        .map(|task| match task {
            crate::timeline::Task::Work(work) => (work.script, work.message),
            _ => panic!("a phase queues only script work"),
        })
        .collect()
}

#[test]
fn a_script_waiting_for_start_with_no_per_frame_messages_starts_in_the_next_phase() {
    let mut world = world();
    let (_, script) = ready_script(&mut world, "Idle");
    world
        .set_script_messages(script, ScriptMessages::NONE)
        .unwrap();
    assert_eq!(
        queued_scripts(&mut world, ScriptMessage::FixedUpdate),
        vec![(script, ScriptMessage::Start)]
    );
    world.lifecycle.mark_started(script);
    for message in &MESSAGES[..4] {
        assert!(
            queued_scripts(&mut world, *message).is_empty(),
            "{message:?}"
        );
    }
}

#[test]
fn only_scripts_that_define_a_message_are_queued_for_it() {
    let mut world = world();
    let (_, late) = ready_script(&mut world, "Late");
    let (_, every) = ready_script(&mut world, "Every");
    world
        .set_script_messages(late, ScriptMessages::LATE_UPDATE)
        .unwrap();
    start_all(&mut world);
    assert_eq!(
        queued_scripts(&mut world, ScriptMessage::LateUpdate),
        vec![
            (late, ScriptMessage::LateUpdate),
            (every, ScriptMessage::LateUpdate)
        ]
    );
    assert_eq!(
        queued_scripts(&mut world, ScriptMessage::Update),
        vec![(every, ScriptMessage::Update)]
    );
}

#[test]
fn setting_a_scripts_messages_moves_it_between_phases() {
    let mut world = world();
    let (_, early) = ready_script(&mut world, "Actor");
    let (_, late) = ready_script(&mut world, "Actor");
    start_all(&mut world);
    world
        .set_script_messages(early, ScriptMessages::FIXED_UPDATE)
        .unwrap();
    assert_phases_match(&mut world);
    world
        .set_script_messages(late, ScriptMessages::UPDATE)
        .unwrap();
    assert_phases_match(&mut world);
    world
        .set_script_messages(early, ScriptMessages::NONE)
        .unwrap();
    assert_phases_match(&mut world);
    world
        .set_script_messages(early, ScriptMessages::ALL)
        .unwrap();
    assert_phases_match(&mut world);
    world.lifecycle.assert_index_matches_scripts();
}

#[test]
fn scripts_of_one_type_keep_separate_sets() {
    let mut world = world();
    let (_, first) = ready_script(&mut world, "Actor");
    let (_, second) = ready_script(&mut world, "Actor");
    start_all(&mut world);
    world
        .set_script_messages(first, ScriptMessages::UPDATE)
        .unwrap();
    assert_eq!(
        queued_scripts(&mut world, ScriptMessage::Update),
        vec![
            (first, ScriptMessage::Update),
            (second, ScriptMessage::Update)
        ]
    );
    assert_eq!(
        queued_scripts(&mut world, ScriptMessage::LateUpdate),
        vec![(second, ScriptMessage::LateUpdate)]
    );
}

#[test]
fn rolling_back_a_staged_script_leaves_the_index_as_before() {
    let mut world = world();
    let (object, kept) = ready_script(&mut world, "Actor");
    world
        .set_script_messages(kept, ScriptMessages::UPDATE)
        .unwrap();
    start_all(&mut world);
    let before = queued_scripts(&mut world, ScriptMessage::Update);
    world.stage_for_test();
    let staged = world.add_component(object, desc("Actor")).unwrap();
    world
        .set_script_messages(staged, ScriptMessages::NONE)
        .unwrap();
    world.roll_back_for_test();
    assert!(
        world
            .set_script_messages(staged, ScriptMessages::ALL)
            .is_err()
    );
    assert_eq!(queued_scripts(&mut world, ScriptMessage::Update), before);
    assert_eq!(
        world.lifecycle.scripts[&kept].messages,
        ScriptMessages::UPDATE
    );
    world.lifecycle.assert_index_matches_scripts();
    assert_phases_match(&mut world);
}

#[test]
fn disabling_and_enabling_a_script_changes_what_is_queued() {
    let mut world = world();
    let (_, script) = ready_script(&mut world, "Actor");
    start_all(&mut world);
    world.set_script_enabled(script, false).unwrap();
    assert!(queued_scripts(&mut world, ScriptMessage::Update).is_empty());
    assert_phases_match(&mut world);
    world.set_script_enabled(script, true).unwrap();
    assert_eq!(
        queued_scripts(&mut world, ScriptMessage::Update),
        vec![(script, ScriptMessage::Update)]
    );
    assert_phases_match(&mut world);
}

#[test]
fn clones_wait_for_start_again_and_removed_scripts_are_not_queued() {
    let mut world = world();
    let (template, original) = ready_script(&mut world, "Actor");
    start_all(&mut world);
    let clone = world.instantiate(template, None).unwrap();
    world.take_lifecycle_work(clone.mutation).unwrap();
    assert_phases_match(&mut world);
    world.lifecycle.assert_index_matches_scripts();
    world.remove_component(original).unwrap();
    assert_phases_match(&mut world);
    world.lifecycle.assert_index_matches_scripts();
    world.destroy(clone.root, DestroyWhen::EndOfFrame).unwrap();
    world.take_cleanup();
    world.finish_phase();
    assert_phases_match(&mut world);
    world.lifecycle.assert_index_matches_scripts();
}

struct Walk {
    world: World,
    objects: Vec<ObjectHandle>,
    state: u64,
}

impl Walk {
    fn next(&mut self, bound: usize) -> usize {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.state >> 33).unwrap() % bound
    }

    fn script(&mut self) -> Option<ComponentHandle> {
        let count = self.world.lifecycle.scripts.len();
        if count == 0 {
            return None;
        }
        let at = self.next(count);
        self.world.lifecycle.scripts.keys().nth(at).copied()
    }

    fn object(&mut self) -> Option<ObjectHandle> {
        self.objects
            .retain(|&object| self.world.object_alive(object));
        if self.objects.is_empty() {
            return None;
        }
        let at = self.next(self.objects.len());
        Some(self.objects[at])
    }

    fn step(&mut self) {
        let type_name = format!("T{}", self.next(3));
        match self.next(10) {
            0 => {
                let object = self.world.create_object(ObjectDesc::default()).unwrap();
                let script = self.world.add_component(object, desc(&type_name)).unwrap();
                self.objects.push(object);
                if self.next(4) != 0 {
                    self.world.set_script_enabled(script, true).unwrap();
                }
            }
            1 => {
                let bits = u32::try_from(self.next(8)).unwrap();
                let messages = ScriptMessages::from_bits(bits).unwrap();
                if let Some(script) = self.script() {
                    self.world.set_script_messages(script, messages).unwrap();
                }
            }
            2 => self.clone_object(),
            3 | 4 => self.toggle(),
            5 => self.remove_script(),
            6 => self.destroy_object(),
            8 | 9 => self.toggle_object(),
            _ => start_all(&mut self.world),
        }
    }

    fn clone_object(&mut self) {
        if let Some(object) = self.object() {
            let clone = self.world.instantiate(object, None).unwrap();
            self.world.take_lifecycle_work(clone.mutation).unwrap();
            self.objects.push(clone.root);
        }
    }

    fn toggle(&mut self) {
        if let Some(script) = self.script() {
            let enabled = self.next(2) == 0;
            self.world.set_script_enabled(script, enabled).unwrap();
        }
    }

    fn remove_script(&mut self) {
        if let Some(script) = self.script() {
            self.world.remove_component(script).unwrap();
        }
    }

    fn toggle_object(&mut self) {
        if let Some(object) = self.object() {
            let active = self.next(2) == 0;
            let mutation = self.world.set_active(object, active).unwrap();
            self.world.take_lifecycle_work(mutation).unwrap();
        }
    }

    fn destroy_object(&mut self) {
        if let Some(object) = self.object() {
            self.world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
            self.world.take_cleanup();
            self.world.finish_phase();
        }
    }
}

#[test]
fn a_long_mixed_history_queues_what_the_full_scan_queues() {
    for seed in 0..6 {
        let mut walk = Walk {
            world: world(),
            objects: Vec::new(),
            state: seed,
        };
        for _ in 0..400 {
            walk.step();
            assert_phases_match(&mut walk.world);
            walk.world.lifecycle.assert_index_matches_scripts();
        }
    }
}

#[test]
fn registering_a_script_again_replaces_its_index_entries() {
    use crate::lifecycle::{ScriptState, StartState};
    let mut world = world();
    let (_, script) = ready_script(&mut world, "Actor");
    world.lifecycle.register(
        script,
        ScriptState {
            start: StartState::Started,
            messages: ScriptMessages::UPDATE,
            ..Default::default()
        },
    );
    world.lifecycle.assert_index_matches_scripts();
    world.lifecycle.register(script, ScriptState::default());
    world.lifecycle.assert_index_matches_scripts();
}

#[test]
fn scripts_that_cannot_be_ready_are_not_candidates_until_they_can() {
    let mut world = world();
    let object = world
        .create_object(ObjectDesc {
            active: false,
            ..Default::default()
        })
        .unwrap();
    let script = world.add_component(object, desc("Idle")).unwrap();
    world
        .set_script_messages(script, ScriptMessages::NONE)
        .unwrap();
    world.set_script_enabled(script, true).unwrap();
    for message in [ScriptMessage::Start, ScriptMessage::FixedUpdate] {
        assert!(world.lifecycle.phase_candidates(message).is_empty());
    }
    let mutation = world.set_active(object, true).unwrap();
    world.take_lifecycle_work(mutation).unwrap();
    for message in [ScriptMessage::Start, ScriptMessage::FixedUpdate] {
        assert_eq!(world.lifecycle.phase_candidates(message), vec![script]);
    }
    let mutation = world.set_active(object, false).unwrap();
    world.take_lifecycle_work(mutation).unwrap();
    assert!(
        world
            .lifecycle
            .phase_candidates(ScriptMessage::Start)
            .is_empty()
    );
    world.lifecycle.assert_index_matches_scripts();
}

#[test]
fn a_script_that_is_not_awake_is_not_a_candidate_even_when_marked_active() {
    let mut world = world();
    let (_, script) = ready_script(&mut world, "Actor");
    assert_eq!(
        world.lifecycle.phase_candidates(ScriptMessage::Start),
        vec![script]
    );
    world
        .lifecycle
        .edit_script(script, |state| state.awake = false);
    assert!(
        world
            .lifecycle
            .phase_candidates(ScriptMessage::Start)
            .is_empty()
    );
    world.lifecycle.assert_index_matches_scripts();
}

#[test]
fn a_script_pending_destruction_leaves_the_start_candidates_at_once() {
    let mut world = world();
    let (object, script) = ready_script(&mut world, "Actor");
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    world.lifecycle.assert_index_matches_scripts();
    assert!(
        world
            .lifecycle
            .phase_candidates(ScriptMessage::Start)
            .is_empty()
    );
    assert!(world.lifecycle.scripts.contains_key(&script));
    assert_phases_match(&mut world);
}
