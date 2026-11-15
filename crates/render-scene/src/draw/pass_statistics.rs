use super::{DrawCount, SceneGpu};
use std::{cell::Cell, rc::Rc};

/// CPU-encoded draws in native UI, terrain, water and background sky passes,
/// the main camera's object passes and each directional shadow cascade.
/// Celestial passes are excluded. Terrain counts the main camera's colour pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassDrawStatistics {
    pub ui: DrawCount,
    /// Subset of UI calls with scene-depth testing enabled.
    pub ui_depth_tested: DrawCount,
    pub terrain: DrawCount,
    pub water: DrawCount,
    pub sky: DrawCount,
    pub shadow_cascades: [DrawCount; 4],
    /// Objects the main camera's depth/normal prepass recorded.
    pub prepass_objects: ObjectDraws,
    /// Objects the main camera's colour pass recorded.
    pub opaque_objects: ObjectDraws,
}

/// The indexed object draws one pass recorded: one call per material slot of each drawn
/// level, including the preceding level's slots for an LOD transition. A draw counts as an
/// impostor when its material is one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectDraws {
    pub meshes: DrawCount,
    pub impostors: DrawCount,
    pub recording: ObjectRecording,
}

/// How a pass recorded its selected objects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectRecording {
    /// The pass did not run in the submitted frame.
    #[default]
    NotRecorded,
    /// One draw per (batch, LOD) group from the view's gathered instances.
    Compacted,
    /// One draw per selected instance range, because the view's gathered instances did not
    /// hold this selection.
    Ranged,
}

impl ObjectDraws {
    pub(crate) fn new(recording: ObjectRecording) -> Self {
        Self {
            recording,
            ..Self::default()
        }
    }

    pub(crate) fn draw(&mut self, impostor: bool, instances: u64) {
        if impostor {
            self.impostors.draw(instances);
        } else {
            self.meshes.draw(instances);
        }
    }

    /// Calls and instances of both kinds.
    pub(crate) fn total(&self) -> DrawCount {
        let mut total = self.meshes;
        total.add(self.impostors);
        total
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PassCounter<T: Copy = DrawCount>(Rc<Cell<T>>);
impl<T: Copy + 'static> PassCounter<T> {
    pub(crate) fn record(&self, count: T) {
        if let Some(mut owner) = worldrunner_render_gpu::submission::Publications::current() {
            let destination = self.0.clone();
            let _ = owner.stage_write(
                crate::compositor::publication::key(&destination),
                move || {
                    destination.set(count);
                },
            );
        }
    }
    fn get(&self) -> T {
        self.0.get()
    }
    fn key(&self) -> worldrunner_render_gpu::submission::capture::WriteKey {
        crate::compositor::publication::key(&self.0)
    }
}
impl PassCounter {
    pub(crate) fn add_recorded(&self, count: DrawCount) {
        if let Some(mut owner) = worldrunner_render_gpu::submission::Publications::current() {
            let destination = self.0.clone();
            let _ = owner.stage_write(
                crate::compositor::publication::key(&destination),
                move || {
                    let mut combined = destination.get();
                    combined.add(count);
                    destination.set(combined);
                },
            );
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct PassCounters {
    pub geometry: Rc<Cell<Option<super::TerrainGeometryStatistics>>>,
    pub ui: PassCounter,
    pub ui_depth_tested: PassCounter,
    pub terrain: PassCounter,
    pub water: PassCounter,
    pub sky: PassCounter,
    pub shadows: [PassCounter; 4],
    pub prepass_objects: PassCounter<ObjectDraws>,
    pub opaque_objects: PassCounter<ObjectDraws>,
}
impl PassCounters {
    pub fn write_keys(
        &self,
    ) -> impl Iterator<Item = worldrunner_render_gpu::submission::capture::WriteKey> + '_ {
        [
            &self.ui,
            &self.ui_depth_tested,
            &self.terrain,
            &self.water,
            &self.sky,
        ]
        .into_iter()
        .chain(&self.shadows)
        .map(PassCounter::key)
        .chain([self.prepass_objects.key(), self.opaque_objects.key()])
    }
    pub fn snapshot(&self) -> PassDrawStatistics {
        PassDrawStatistics {
            ui: self.ui.get(),
            ui_depth_tested: self.ui_depth_tested.get(),
            terrain: self.terrain.get(),
            water: self.water.get(),
            sky: self.sky.get(),
            shadow_cascades: self.shadows.each_ref().map(PassCounter::get),
            prepass_objects: self.prepass_objects.get(),
            opaque_objects: self.opaque_objects.get(),
        }
    }
}
impl SceneGpu {
    /// Clears the frame's pass counters, keeping prepass counts when that pass already ran
    /// in this frame or a submitted geometry prefix.
    pub(crate) fn reset_pass_statistics(&self, keep_prepass: bool) {
        for counter in [
            &self.pass_counters.ui,
            &self.pass_counters.ui_depth_tested,
            &self.pass_counters.terrain,
            &self.pass_counters.water,
            &self.pass_counters.sky,
        ]
        .into_iter()
        .chain(&self.pass_counters.shadows)
        {
            counter.record(DrawCount::default());
        }
        if !keep_prepass {
            self.pass_counters
                .prepass_objects
                .record(ObjectDraws::default());
        }
        self.pass_counters
            .opaque_objects
            .record(ObjectDraws::default());
    }
}
impl DrawCount {
    pub(crate) fn draw(&mut self, instances: u64) {
        self.calls += 1;
        self.instances += instances;
    }
    pub(crate) fn add(&mut self, other: Self) {
        self.calls += other.calls;
        self.instances += other.instances;
    }
}

#[cfg(test)]
mod tests {
    use super::{DrawCount, ObjectDraws, ObjectRecording, PassCounters};

    #[test]
    fn object_draws_count_each_call_once_under_its_material_kind() {
        let mut draws = ObjectDraws::new(ObjectRecording::Compacted);
        draws.draw(false, 5);
        draws.draw(true, 3);
        draws.draw(false, 2);
        assert_eq!(draws.recording, ObjectRecording::Compacted);
        assert_eq!(
            draws.meshes,
            DrawCount {
                calls: 2,
                instances: 7
            }
        );
        assert_eq!(
            draws.impostors,
            DrawCount {
                calls: 1,
                instances: 3
            }
        );
        let total = draws.total();
        assert_eq!(total.calls, draws.meshes.calls + draws.impostors.calls);
        assert_eq!(
            total.instances,
            draws.meshes.instances + draws.impostors.instances
        );
    }

    #[test]
    fn camera_object_counters_declare_their_writes_and_reach_the_snapshot() {
        let counters = PassCounters::default();
        let keys: Vec<_> = counters.write_keys().collect();
        for counter in [&counters.prepass_objects, &counters.opaque_objects] {
            assert!(keys.contains(&counter.key()));
        }
        let mut prepass = ObjectDraws::new(ObjectRecording::Ranged);
        prepass.draw(false, 4);
        let mut opaque = ObjectDraws::new(ObjectRecording::Compacted);
        opaque.draw(true, 4);
        counters.prepass_objects.0.set(prepass);
        counters.opaque_objects.0.set(opaque);
        let snapshot = counters.snapshot();
        assert_eq!(snapshot.prepass_objects, prepass);
        assert_eq!(snapshot.opaque_objects, opaque);
        assert_eq!(
            ObjectDraws::default().recording,
            ObjectRecording::NotRecorded
        );
    }
}
