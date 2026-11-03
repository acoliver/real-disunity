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
    world
        .set_script_type_messages("Example", "Idle", ScriptMessages::NONE)
        .unwrap();
    let (_, script) = ready_script(&mut world, "Idle");
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
    world
        .set_script_type_messages("Example", "Late", ScriptMessages::LATE_UPDATE)
        .unwrap();
    let (_, late) = ready_script(&mut world, "Late");
    let (_, every) = ready_script(&mut world, "Every");
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
fn reports_before_and_after_registration_move_scripts_between_phases() {
    let mut world = world();
    world
        .set_script_type_messages("Example", "Early", ScriptMessages::FIXED_UPDATE)
        .unwrap();
    ready_script(&mut world, "Early");
    ready_script(&mut world, "Late");
    start_all(&mut world);
    assert_phases_match(&mut world);
    world
        .set_script_type_messages("Example", "Late", ScriptMessages::UPDATE)
        .unwrap();
    assert_phases_match(&mut world);
    world
        .set_script_type_messages("Example", "Early", ScriptMessages::NONE)
        .unwrap();
    assert_phases_match(&mut world);
    world
        .set_script_type_messages("Example", "Early", ScriptMessages::ALL)
        .unwrap();
    assert_phases_match(&mut world);
    world.lifecycle.assert_index_matches_scripts();
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
        match self.next(8) {
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
                self.world
                    .set_script_type_messages("Example", &type_name, messages)
                    .unwrap();
            }
            2 => self.clone_object(),
            3 | 4 => self.toggle(),
            5 => self.remove_script(),
            6 => self.destroy_object(),
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
