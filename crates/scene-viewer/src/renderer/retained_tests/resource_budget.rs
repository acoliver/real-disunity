//! The presentation budget counts the textures and meshes of a resident scene the same way
//! whether they were uploaded with the scene or appended to it, and a full rebuild reserves the
//! rebuilt resource set as a whole.
use super::resource_edits::large_texture;
use super::*;
use crate::presentation::Presentation;
use crate::renderer::readiness::Streaming;
use crate::retained::resource_edits::tests::{append, material_on};
use crate::{RetainedHost, RetainedUpdate};
use worldrunner_render_plan::SceneAppend;

struct Budgeted {
    context: GpuContext,
    host: RetainedHost,
    resident: Resident,
    streaming: Streaming,
    presentation: Presentation,
    baseline: u64,
}

fn scene_bytes_of(context: &GpuContext) -> u64 {
    let mut resident =
        Resident::new(context, &source(), None, crate::FrameProfile::Legacy).unwrap();
    resident_payload(&mut resident)
}

fn resident_payload(resident: &mut Resident) -> u64 {
    let statistics = resident.scene_mut().statistics();
    statistics.textures.resident_bytes + statistics.mesh_buffers.resident_bytes
}

/// A presentation configured for the scene's effects, so a rebuild that applies them leaves
/// the targets alone, with `headroom` bytes of budget above its own storage.
fn presentation(context: &GpuContext, headroom: u64) -> Presentation {
    let size = Viewport {
        width: 64,
        height: 64,
    };
    let make = |budget: u64| {
        Presentation::with_settings(
            context,
            COLOR,
            [size, size],
            crate::FrameProfile::Legacy,
            source().effects(),
            &crate::presentation::PresentationPolicy {
                budget: crate::PresentationBudget(budget).into(),
                capture_pool: crate::CapturePoolPolicy::default(),
                recipes: false,
            },
        )
        .unwrap()
    };
    let own = make(u64::MAX / 2)
        .scheduler
        .resident_bytes(&context.device().limits())
        .unwrap();
    make(own + headroom)
}

impl Budgeted {
    /// `headroom` is the budget left above the presentation's own storage and the initial scene.
    fn new(headroom: u64) -> Self {
        let context =
            pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
        let initial = scene_bytes_of(&context);
        let mut presentation = presentation(&context, initial + headroom);
        let baseline = presentation
            .scheduler
            .resident_bytes(&context.device().limits())
            .unwrap();
        let resident = super::super::environment::new_resident(
            &context,
            &source(),
            None,
            crate::FrameProfile::Legacy,
            &mut presentation,
        )
        .unwrap();
        Self {
            host: RetainedHost::new(source(), crate::tests::options()).unwrap(),
            streaming: Streaming::new(None, crate::FrameProfile::Legacy),
            context,
            resident,
            presentation,
            baseline,
        }
    }

    fn publish(&mut self) -> Result<(), crate::ViewerError> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while self.host.ready_generation() != self.host.accepted_generation() {
            self.context.device().poll(wgpu::PollType::Poll).unwrap();
            self.streaming.publish(
                &self.context,
                &mut self.resident,
                &mut self.presentation,
                &mut self.host,
            )?;
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        Ok(())
    }

    fn apply(&mut self, update: RetainedUpdate) -> Result<(), crate::ViewerError> {
        self.host.apply(update).unwrap();
        self.publish()
    }

    fn rebuild(&mut self) {
        let scene = self.host.scene();
        let update = RetainedUpdate::Resources {
            resources: scene.resources().clone(),
            meshes: scene.geometry().meshes().to_vec(),
        };
        self.apply(update).unwrap();
    }

    /// Storage the presentation budget holds for scenes, beyond its own targets.
    fn reserved(&self) -> u64 {
        self.presentation
            .scheduler
            .resident_bytes(&self.context.device().limits())
            .unwrap()
            - self.baseline
    }

    fn resident(&mut self) -> u64 {
        resident_payload(&mut self.resident)
    }

    fn retire(&mut self) {
        self.context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        self.streaming.collect(u64::MAX);
        self.presentation.scheduler.collect();
    }
}

#[test]
fn reserved_scene_bytes_equal_the_resident_bytes_through_append_rebuild_append() {
    let mut fixture = Budgeted::new(256 * 1024 * 1024);
    let initial = fixture.resident();
    assert_ne!(initial, 0);
    assert_eq!(fixture.reserved(), initial, "the scene as first uploaded");

    fixture
        .apply(RetainedUpdate::AppendResources(append()))
        .unwrap();
    let appended = fixture.resident();
    assert!(appended > initial);
    assert_eq!(fixture.reserved(), appended, "after the first append");

    fixture.rebuild();
    assert_eq!(fixture.resident(), appended);
    assert!(
        fixture.reserved() >= 2 * appended,
        "the retired resident stays reserved until the queue is done with it"
    );
    fixture.retire();
    assert_eq!(fixture.reserved(), appended, "after the rebuild retires");

    fixture
        .apply(RetainedUpdate::AppendResources(SceneAppend {
            textures: vec![large_texture()],
            materials: vec![material_on(2)],
            meshes: Vec::new(),
        }))
        .unwrap();
    let again = fixture.resident();
    assert!(again > appended);
    assert_eq!(fixture.reserved(), again, "after the append that follows");
}

#[test]
fn appends_after_a_rebuild_are_admitted_against_the_whole_rebuilt_set() {
    const MIB: u64 = 1024 * 1024;
    let mut fixture = Budgeted::new(9 * MIB);
    let large = |handle| SceneAppend {
        textures: vec![large_texture()],
        materials: vec![material_on(handle)],
        meshes: Vec::new(),
    };
    fixture
        .apply(RetainedUpdate::AppendResources(large(1)))
        .unwrap();
    fixture.rebuild();
    fixture.retire();
    fixture
        .apply(RetainedUpdate::AppendResources(large(2)))
        .unwrap();
    let base = fixture.resident.scene_mut().resource_base();
    let reserved = fixture.reserved();
    let error = fixture
        .apply(RetainedUpdate::AppendResources(large(3)))
        .unwrap_err();
    assert!(
        matches!(
            error,
            crate::ViewerError::Configuration(
                worldrunner_render_gpu::configuration::ConfigurationError::Budget { .. }
            )
        ),
        "{error}"
    );
    assert_eq!(fixture.resident.scene_mut().resource_base(), base);
    assert_eq!(
        fixture.reserved(),
        reserved,
        "a refused append reserves nothing"
    );
}
