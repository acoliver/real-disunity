//! What keeps the renderer's reservation and the runtime's accounted bytes held: live
//! resources, admitted draws that can still be recorded, and the frames that retain the arena.

use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_gpu::submission::LeaseError;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{Geometry, ProgramArena, ProgramRuntime};

use crate::arena::{ARENA, Draw, admit, admit_geometry, arena_runtime, draw};
use crate::common::Gpu;
use crate::surface::{Surface, pixel, read};

const RGBA8: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The renderer's configuration, which counts the arenas it reserved while they are held.
struct Renderer {
    configuration: Configuration<()>,
    limits: wgpu::Limits,
}

impl Renderer {
    fn new(gpu: &Gpu) -> Self {
        let limits = gpu.device.limits();
        let configuration = Configuration::new(
            &gpu.authority,
            (),
            AllocationSet::default(),
            &limits,
            AllocationBudget(1 << 20),
        )
        .unwrap();
        Self {
            configuration,
            limits,
        }
    }

    fn arena(&mut self, runtime: &ProgramRuntime) -> ProgramArena {
        let reservation = self
            .configuration
            .reserve::<()>(&self.limits, ProgramRuntime::arena_allocations(ARENA))
            .unwrap();
        runtime.open_arena(reservation).unwrap()
    }

    /// Bytes the renderer counts as reserved now.
    fn held(&self) -> u64 {
        self.configuration.resident_bytes(&self.limits).unwrap()
    }
}

fn refill(runtime: &mut ProgramRuntime) -> Result<(), ProgramError> {
    let all = vec![0; usize::try_from(ARENA).unwrap()];
    runtime.create_buffer("refill", &all).map(|_| ())
}

fn release(runtime: &mut ProgramRuntime, draw: &Draw) {
    assert!(runtime.release_buffer(draw.input.buffer).unwrap());
    for binding in &draw.bindings {
        let worldrunner_render_program::runtime::Resource::Buffer(buffer) = binding.resource else {
            panic!("the patch binds buffers only");
        };
        assert!(runtime.release_buffer(buffer).unwrap());
    }
}

#[test]
fn a_retired_arena_keeps_the_renderers_reservation_until_the_runtimes_resources_are_gone() {
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu);
    let mut runtime = gpu.runtime_with(AllocationBudget(ARENA));
    let mut arena = renderer.arena(&runtime);
    let reserved = renderer.held();
    assert_eq!(reserved, ARENA);
    let buffer = runtime.create_buffer("live", &[0; 16]).unwrap();

    arena.retire().unwrap();
    assert_eq!(
        renderer.held(),
        reserved,
        "a live buffer still counts against the reservation"
    );
    assert_eq!(
        runtime.create_buffer("late", &[0; 16]).unwrap_err(),
        ProgramError::Lease(LeaseError::Expired),
        "nothing new is admitted once the reservation is being given back"
    );

    assert!(runtime.release_buffer(buffer).unwrap());
    runtime.collect();
    assert_eq!(renderer.held(), 0, "the last resource gave it back");
}

#[test]
fn a_dropped_arena_keeps_the_renderers_reservation_until_the_runtimes_resources_are_gone() {
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu);
    let mut runtime = gpu.runtime_with(AllocationBudget(ARENA));
    let arena = renderer.arena(&runtime);
    let reserved = renderer.held();
    let buffer = runtime.create_buffer("live", &[0; 16]).unwrap();

    drop(arena);
    assert_eq!(renderer.held(), reserved);
    assert_eq!(
        runtime.create_buffer("late", &[0; 16]).unwrap_err(),
        ProgramError::Lease(LeaseError::Expired)
    );

    assert!(runtime.release_buffer(buffer).unwrap());
    runtime.collect();
    assert_eq!(renderer.held(), 0);
}

#[test]
fn a_reservation_with_nothing_live_is_given_back_when_the_arena_retires() {
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu);
    let runtime = gpu.runtime_with(AllocationBudget(ARENA));
    let mut arena = renderer.arena(&runtime);
    arena.retire().unwrap();
    assert_eq!(renderer.held(), 0);
}

#[test]
fn a_newer_arena_does_not_end_what_an_older_one_admitted() {
    let gpu = Gpu::new();
    let mut renderer = Renderer::new(&gpu);
    let mut runtime = gpu.runtime_with(AllocationBudget(ARENA));
    let first = renderer.arena(&runtime);
    let second = renderer.arena(&runtime);
    drop(first);
    runtime
        .create_buffer("under the second", &[0; 16])
        .expect("the runtime allocates under the arena that is still open");
    drop(second);
}

#[test]
fn released_resources_stay_counted_while_an_admitted_draw_can_still_use_them() {
    let gpu = Gpu::new();
    let (mut runtime, arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let surface = Surface::new(&gpu.device, RGBA8);

    let frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = admit(&mut runtime, &frame, &arena, &draw).unwrap();
    frame.finish().cancel();
    release(&mut runtime, &draw);
    runtime.collect();
    assert!(
        matches!(refill(&mut runtime), Err(ProgramError::OverBudget { .. })),
        "the buffers the admitted draw holds are still counted"
    );

    // The draw is still valid in a later frame.
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    admitted.retain(&frame).unwrap();
    let mut pass = surface.open(&mut frame);
    admitted.record(&mut pass);
    drop(pass);
    let readback = surface.copy_color(&gpu.device, &mut frame);
    frame.finish().submit().unwrap();
    let bytes = read(&gpu.device, &readback);
    assert_eq!(pixel(&bytes, 32, 32), [0, 255, 0, 255]);

    drop(admitted);
    runtime.collect();
    refill(&mut runtime).expect("the completed frame and the dropped admission freed the bytes");
}

#[test]
fn a_draw_dropped_while_its_frame_is_unsubmitted_is_counted_until_that_frame_completes() {
    let gpu = Gpu::new();
    let (mut runtime, arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let surface = Surface::new(&gpu.device, RGBA8);

    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = admit(&mut runtime, &frame, &arena, &draw).unwrap();
    let mut pass = surface.open(&mut frame);
    admitted.record(&mut pass);
    drop(pass);
    drop(admitted);
    let readback = surface.copy_color(&gpu.device, &mut frame);
    let serial = gpu.authority.submitted_serial() + 1;
    frame.finish().submit().unwrap();
    assert_eq!(gpu.authority.submitted_serial(), serial);

    release(&mut runtime, &draw);
    let bytes = read(&gpu.device, &readback);
    assert_eq!(pixel(&bytes, 32, 32), [0, 255, 0, 255]);
    runtime.collect();
    refill(&mut runtime).expect("the frame completed, so nothing holds the bytes");
}

#[test]
fn a_dropped_arena_fails_the_retain_of_a_frame_that_already_holds_it() {
    let gpu = Gpu::new();
    let (mut runtime, arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = admit(&mut runtime, &frame, &arena, &draw).unwrap();
    drop(arena);
    assert_eq!(
        admitted.retain(&frame).unwrap_err(),
        ProgramError::Lease(LeaseError::Expired),
        "the frame's own retention of the arena does not hide that it is gone"
    );
}

#[test]
fn admission_refuses_ranges_past_the_bound_buffers_by_name_and_admits_nothing() {
    let gpu = Gpu::new();
    let (mut runtime, mut arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    // The patch's buffer holds six 12-byte vertices; a seventh would end at byte 84.
    let past = admit_geometry(
        &mut runtime,
        &frame,
        (&arena, &draw),
        Geometry::Vertices { first: 0, count: 7 },
    );
    assert_eq!(
        past.unwrap_err(),
        ProgramError::VertexRangePastBuffer {
            slot: 0,
            needed: 84,
            have: 72
        }
    );
    let overflow = admit_geometry(
        &mut runtime,
        &frame,
        (&arena, &draw),
        Geometry::Vertices {
            first: u32::MAX,
            count: 1,
        },
    );
    assert_eq!(
        overflow.unwrap_err(),
        ProgramError::DrawRangeOverflow {
            first: u32::MAX,
            count: 1
        }
    );
    frame.finish().cancel();
    arena
        .retire()
        .expect("a refused admission did not retain the arena");
}
