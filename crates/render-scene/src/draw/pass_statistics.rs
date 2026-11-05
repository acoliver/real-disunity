use super::{DrawCount, SceneGpu};
use std::{cell::Cell, rc::Rc};

/// CPU-encoded draws in native UI, terrain, water and background sky passes,
/// and each directional shadow cascade. Metadata and celestial passes are excluded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassDrawStatistics {
    pub ui: DrawCount,
    pub terrain: DrawCount,
    pub water: DrawCount,
    pub sky: DrawCount,
    pub shadow_cascades: [DrawCount; 4],
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PassCounter(Rc<Cell<DrawCount>>);
impl PassCounter {
    pub(crate) fn record(&self, count: DrawCount) {
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
    fn get(&self) -> DrawCount {
        self.0.get()
    }
}

#[derive(Debug, Default)]
pub(crate) struct PassCounters {
    pub geometry: Rc<Cell<Option<super::TerrainGeometryStatistics>>>,
    pub ui: PassCounter,
    pub terrain: PassCounter,
    pub water: PassCounter,
    pub sky: PassCounter,
    pub shadows: [PassCounter; 4],
}
impl PassCounters {
    pub fn write_keys(
        &self,
    ) -> impl Iterator<Item = worldrunner_render_gpu::submission::capture::WriteKey> + '_ {
        [&self.ui, &self.terrain, &self.water, &self.sky]
            .into_iter()
            .chain(&self.shadows)
            .map(|counter| crate::compositor::publication::key(&counter.0))
    }
    pub fn snapshot(&self) -> PassDrawStatistics {
        PassDrawStatistics {
            ui: self.ui.get(),
            terrain: self.terrain.get(),
            water: self.water.get(),
            sky: self.sky.get(),
            shadow_cascades: self.shadows.each_ref().map(PassCounter::get),
        }
    }
}
impl SceneGpu {
    pub(crate) fn reset_pass_statistics(&self) {
        for counter in [
            &self.pass_counters.ui,
            &self.pass_counters.terrain,
            &self.pass_counters.water,
            &self.pass_counters.sky,
        ]
        .into_iter()
        .chain(&self.pass_counters.shadows)
        {
            counter.record(DrawCount::default());
        }
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
