//! The program runtimes a family of pipelines shares.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use wgpu::Device;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::{ProgramRuntime, RuntimeStats};

/// The program runtimes of a family of pipelines: the pipelines a scene was built with and the
/// siblings it specializes from them. Terrain and objects each have one, so each counts what it
/// builds. Terrain and object pipelines compile once per context however many scenes draw with
/// them. A runtime is made when the first terrain or object is prepared, from that scene's
/// submission authority, and counts nothing against a budget: the resources it draws with are
/// lent by the scene, which has its own accounting.
#[derive(Clone, Default)]
pub(crate) struct Family(Rc<FamilyState>);

struct FamilyState {
    id: u64,
    runtime: RefCell<Option<ProgramRuntime>>,
    /// The runtime of the objects, apart from the terrain's so that each counts what it builds.
    objects: RefCell<Option<ProgramRuntime>>,
}

static FAMILIES: AtomicU64 = AtomicU64::new(0);

impl Default for FamilyState {
    fn default() -> Self {
        Self {
            id: FAMILIES.fetch_add(1, Ordering::Relaxed),
            runtime: RefCell::default(),
            objects: RefCell::default(),
        }
    }
}

/// A family that does not keep its runtime alive.
#[derive(Clone, Debug)]
pub(crate) struct WeakFamily(std::rc::Weak<FamilyState>);

impl WeakFamily {
    pub(crate) fn alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

impl std::fmt::Debug for Family {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Family")
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl Family {
    /// Tells this family from every other, for caches that outlive a context.
    pub(crate) fn id(&self) -> u64 {
        self.0.id
    }

    pub(crate) fn downgrade(&self) -> WeakFamily {
        WeakFamily(Rc::downgrade(&self.0))
    }

    /// What the runtime has built so far.
    pub(crate) fn stats(&self) -> RuntimeStats {
        self.0
            .runtime
            .borrow()
            .as_ref()
            .map_or_else(RuntimeStats::default, ProgramRuntime::stats)
    }

    /// What the objects' runtime has built so far.
    #[cfg(test)]
    pub(crate) fn object_stats(&self) -> RuntimeStats {
        self.0
            .objects
            .borrow()
            .as_ref()
            .map_or_else(RuntimeStats::default, ProgramRuntime::stats)
    }

    /// Runs `build` against the terrain's runtime, making it from `authority` first if no scene
    /// has.
    pub(crate) fn with<R>(
        &self,
        target: (&Device, &SubmissionAuthority),
        build: impl FnOnce(&mut ProgramRuntime) -> R,
    ) -> R {
        Self::run(&self.0.runtime, target, build)
    }

    /// Runs `build` against the objects' runtime, making it from `authority` first if no scene
    /// has.
    pub(crate) fn with_objects<R>(
        &self,
        target: (&Device, &SubmissionAuthority),
        build: impl FnOnce(&mut ProgramRuntime) -> R,
    ) -> R {
        Self::run(&self.0.objects, target, build)
    }

    fn run<R>(
        slot: &RefCell<Option<ProgramRuntime>>,
        (device, authority): (&Device, &SubmissionAuthority),
        build: impl FnOnce(&mut ProgramRuntime) -> R,
    ) -> R {
        let mut runtime = slot.borrow_mut();
        let runtime = runtime.get_or_insert_with(|| {
            ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(0))
        });
        build(runtime)
    }
}
