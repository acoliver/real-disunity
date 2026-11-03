//! A seeded random history of world mutations. Every public mutation that can change a script's
//! activity, enabled state or awake eligibility is driven, between frames and inside callbacks.

use super::reference::assert_frame_start_matches;
use crate::{
    Advance, CallbackOutcome, ComponentDesc, ComponentHandle, Construction, DestroyWhen,
    ObjectDesc, ObjectHandle, ParentSpace, ScriptMessage, TimeSpan, WaitCondition, World,
    WorldSettings,
};

const MAX_OBJECTS: usize = 40;

pub(super) struct Walk {
    pub world: World,
    objects: Vec<ObjectHandle>,
    open: Vec<Construction>,
    state: u64,
}

fn script_desc(type_name: usize) -> ComponentDesc {
    ComponentDesc::Script {
        assembly: "Example".into(),
        type_name: format!("T{type_name}"),
    }
}

impl Walk {
    pub fn new(seed: u64) -> Self {
        Self {
            world: World::new(WorldSettings::default()).unwrap(),
            objects: Vec::new(),
            open: Vec::new(),
            state: seed,
        }
    }

    pub fn next(&mut self, bound: usize) -> usize {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.state >> 33).unwrap() % bound
    }

    fn chance(&mut self, in_n: usize) -> bool {
        self.next(in_n) == 0
    }

    fn object(&mut self) -> Option<ObjectHandle> {
        let world = &self.world;
        self.objects.retain(|&object| world.object_alive(object));
        if self.objects.is_empty() {
            return None;
        }
        let at = self.next(self.objects.len());
        Some(self.objects[at])
    }

    fn script(&mut self) -> Option<ComponentHandle> {
        let count = self.world.lifecycle.scripts.len();
        if count == 0 {
            return None;
        }
        let at = self.next(count);
        self.world.lifecycle.scripts.keys().nth(at).copied()
    }

    fn construction(&mut self) -> Option<Construction> {
        if self.open.is_empty() {
            return None;
        }
        let at = self.next(self.open.len());
        Some(self.open[at])
    }

    fn drop_construction(&mut self, construction: Construction) {
        self.open.retain(|open| *open != construction);
    }

    /// One mutation that is legal both between frames and inside a callback.
    pub fn mutate(&mut self) {
        match self.next(18) {
            0 | 1 => self.new_scripted_object(),
            2 => self.add_script(),
            3 => self.remove_script(),
            4 | 5 => self.set_enabled(),
            6 | 7 => self.set_active(),
            8 | 9 => self.reparent(),
            10 => self.instantiate(),
            11 => self.begin_construction(),
            12 => self.build_in_construction(),
            13 => self.publish_construction(),
            14 => self.destroy_pending(),
            15 => self.roll_back_script(),
            16 => self.wake(),
            _ => self.start_coroutine(),
        }
    }

    /// Whether the world is small enough to add objects; cloning grows it geometrically and every
    /// callback is checked against a full copy.
    fn room(&self) -> bool {
        self.world.storage_counts().0 < MAX_OBJECTS
    }

    fn new_scripted_object(&mut self) {
        if !self.room() {
            return;
        }
        let active = !self.chance(4);
        let desc = ObjectDesc {
            active,
            ..Default::default()
        };
        let object = self.world.create_object(desc).unwrap();
        self.objects.push(object);
        let type_name = self.next(3);
        let script = self.world.add_component(object, script_desc(type_name));
        if self.next(4) != 0 {
            self.world
                .set_script_enabled(script.unwrap(), true)
                .unwrap();
        }
    }

    fn add_script(&mut self) {
        if let Some(object) = self.object() {
            let type_name = self.next(3);
            let _ = self.world.add_component(object, script_desc(type_name));
        }
    }

    fn remove_script(&mut self) {
        if let Some(script) = self.script() {
            if self.chance(2) {
                let _ = self.world.remove_component(script);
            } else {
                let _ = self.world.destroy_component(script);
            }
        }
    }

    fn set_enabled(&mut self) {
        if let Some(script) = self.script() {
            let enabled = self.chance(2);
            let _ = self.world.set_script_enabled(script, enabled);
        }
    }

    fn set_active(&mut self) {
        if let Some(object) = self.object() {
            let active = self.chance(2);
            let _ = self.world.set_active(object, active);
        }
    }

    fn reparent(&mut self) {
        let Some(object) = self.object() else { return };
        let parent = if self.chance(4) { None } else { self.object() };
        let _ = self
            .world
            .set_parent(object, parent, ParentSpace::KeepLocal);
    }

    fn instantiate(&mut self) {
        if !self.room() {
            return;
        }
        let Some(template) = self.object() else {
            return;
        };
        let parent = if self.chance(2) { self.object() } else { None };
        if self.chance(2) {
            if let Ok(clone) = self.world.instantiate(template, parent) {
                self.objects.extend(clone.objects.values().copied());
            }
        } else if let Ok(clone) = self.world.instantiate_unpublished(template, parent) {
            self.objects.extend(clone.objects.values().copied());
            self.open.push(clone.construction());
        }
    }

    fn begin_construction(&mut self) {
        if let Ok(construction) = self.world.begin_construction() {
            self.open.push(construction);
        }
    }

    fn build_in_construction(&mut self) {
        if !self.room() {
            return;
        }
        let Some(construction) = self.construction() else {
            return;
        };
        let desc = ObjectDesc {
            active: !self.chance(4),
            ..Default::default()
        };
        let Ok(object) = self.world.create_object_in(construction, desc) else {
            return;
        };
        self.objects.push(object);
        let type_name = self.next(3);
        let script = self
            .world
            .add_component(object, script_desc(type_name))
            .unwrap();
        if let Some(parent) = self.object().filter(|_| self.chance(2)) {
            let _ = self
                .world
                .set_parent(object, Some(parent), ParentSpace::KeepLocal);
        }
        if self.next(4) != 0 {
            self.world.set_script_enabled(script, true).unwrap();
        }
    }

    fn publish_construction(&mut self) {
        let Some(construction) = self.construction() else {
            return;
        };
        if self.chance(4) && self.all_alive(construction) {
            if self.world.discard_construction(construction).is_ok() {
                self.drop_construction(construction);
            }
        } else if self.world.publish_construction(construction).is_ok() {
            self.drop_construction(construction);
        }
    }

    /// Discarding retires the construction's objects by handle and cannot tell one that an
    /// earlier discard already removed from a new object in its slot, so the walk only discards
    /// constructions whose objects are all still there.
    fn all_alive(&self, construction: Construction) -> bool {
        let sequence = construction.mutation().sequence;
        let objects = self
            .world
            .lifecycle
            .unpublished
            .get(sequence)
            .unwrap_or(&[]);
        objects
            .iter()
            .all(|&object| self.world.object_alive(object))
    }

    fn destroy_pending(&mut self) {
        if let Some(object) = self.object() {
            let _ = self.world.destroy(object, DestroyWhen::EndOfFrame);
        }
    }

    fn roll_back_script(&mut self) {
        let Some(object) = self.object() else { return };
        self.world.stage_for_test();
        let type_name = self.next(3);
        let _ = self.world.add_component(object, script_desc(type_name));
        self.world.roll_back_for_test();
    }

    fn wake(&mut self) {
        if let Some(object) = self.object() {
            let _ = self.world.wake(object);
        }
    }

    fn start_coroutine(&mut self) {
        if let Some(script) = self.script() {
            let _ = self.world.start_coroutine(script, WaitCondition::NextFrame);
        }
    }

    /// A mutation that needs the world idle: retiring objects and scopes opened by the host.
    pub fn idle_mutation(&mut self) {
        match self.next(4) {
            0 => {
                if let Some(object) = self.object() {
                    let _ = self.world.destroy(object, DestroyWhen::EndOfFrame);
                    self.world.take_cleanup();
                    self.world.finish_phase();
                }
            }
            1 => {
                if let Some(object) = self.object() {
                    let delay = TimeSpan::new(1, 50).unwrap();
                    let _ = self.world.destroy(object, DestroyWhen::AfterScaled(delay));
                }
            }
            2 => self.activate_construction(),
            _ => {
                if let Ok(work) = self.world.flush_cleanup() {
                    self.drain(work);
                }
            }
        }
    }

    fn activate_construction(&mut self) {
        let Some(construction) = self.construction() else {
            return;
        };
        if let Ok(work) = self.world.activate_construction(construction) {
            self.drop_construction(construction);
            self.drain(work);
        }
    }

    pub fn advance(&mut self) {
        let step = TimeSpan::new(1 + u64::try_from(self.next(6)).unwrap(), 50).unwrap();
        let work = self.world.advance(step).unwrap();
        self.drain(work);
    }

    /// Resumes callbacks until the scope ends, mutating the world inside some of them.
    fn drain(&mut self, mut work: Advance) {
        loop {
            assert_frame_start_matches(&self.world);
            let (ticket, outcome) = match work {
                Advance::Callback(callback) => {
                    if self.chance(3) {
                        self.mutate();
                    }
                    let coroutine = matches!(callback.message, ScriptMessage::Coroutine(_));
                    let outcome = if coroutine && self.chance(3) {
                        CallbackOutcome::Yielded(WaitCondition::NextFrame)
                    } else {
                        CallbackOutcome::Returned
                    };
                    (callback.ticket, outcome)
                }
                Advance::AssetCallback(asset) => (asset.ticket, CallbackOutcome::Returned),
                Advance::LifecycleComplete | Advance::Complete(_) => return,
            };
            work = self.world.resume(ticket, outcome).unwrap();
        }
    }
}
