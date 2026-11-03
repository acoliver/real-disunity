//! Admitting draws into a frame: the program arena, the authority of the frame and the lifetime
//! of the owner the frame retains.

use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_gpu::submission::LeaseError;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{
    AdmittedPrograms, DrawPacket, Geometry, ProgramArena, ProgramRuntime, TargetSignature,
};

use crate::common::Gpu;
use crate::patch::{Patch, patch, rectangle, state};
use crate::surface::{Surface, pixel, read};

const RGBA8: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const ARENA: u64 = 4096;

pub fn signature() -> TargetSignature {
    TargetSignature::new(&[RGBA8], Some(DEPTH), 1).unwrap()
}

/// A runtime capped at `ARENA` bytes and the arena the renderer's configuration reserved for it.
pub fn arena_runtime(gpu: &Gpu) -> (ProgramRuntime, ProgramArena) {
    let mut runtime = gpu.runtime_with(AllocationBudget(ARENA));
    let arena = open(gpu, &mut runtime, ARENA).unwrap();
    (runtime, arena)
}

fn open(gpu: &Gpu, runtime: &mut ProgramRuntime, bytes: u64) -> Result<ProgramArena, ProgramError> {
    let limits = gpu.device.limits();
    let mut renderer = Configuration::new(
        &gpu.authority,
        (),
        AllocationSet::default(),
        &limits,
        AllocationBudget(1 << 20),
    )
    .unwrap();
    let reservation = renderer
        .reserve::<()>(&limits, ProgramRuntime::arena_allocations(bytes))
        .unwrap();
    runtime.open_arena(reservation)
}

/// Everything one patch draw needs, built in `runtime`.
pub struct Draw {
    pub patch: Patch,
    pub bindings: Vec<worldrunner_render_program::runtime::BoundResource>,
    pub input: worldrunner_render_program::runtime::VertexInput,
}

pub fn draw(runtime: &mut ProgramRuntime) -> Draw {
    let patch = patch(runtime);
    let bindings = patch.color(runtime, [0.0, 1.0, 0.0, 1.0]);
    let input = rectangle(runtime, [-0.5, -0.5, 0.5, 0.5], 0.5);
    Draw {
        patch,
        bindings,
        input,
    }
}

pub fn admit(
    runtime: &mut ProgramRuntime,
    encoder: &wgpu::CommandEncoder,
    arena: &ProgramArena,
    draw: &Draw,
) -> Result<AdmittedPrograms, ProgramError> {
    admit_geometry(
        runtime,
        encoder,
        (arena, draw),
        Geometry::Vertices { first: 0, count: 6 },
    )
}

/// Admits the draw's one packet with `geometry` in place of its six vertices.
pub fn admit_geometry(
    runtime: &mut ProgramRuntime,
    encoder: &wgpu::CommandEncoder,
    (arena, draw): (&ProgramArena, &Draw),
    geometry: Geometry,
) -> Result<AdmittedPrograms, ProgramError> {
    let state = state();
    runtime.admit_draws(
        encoder,
        arena,
        signature(),
        &[DrawPacket {
            program: &draw.patch.program,
            state: &state,
            front_face: wgpu::FrontFace::Cw,
            vertex_inputs: std::slice::from_ref(&draw.input),
            bindings: &draw.bindings,
            geometry,
        }],
    )
}

#[test]
fn admitted_draws_record_into_the_frame_and_hold_the_arena_until_it_completes() {
    let gpu = Gpu::new();
    let (mut runtime, mut arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let surface = Surface::new(&gpu.device, RGBA8);
    let before = gpu.authority.submitted_serial();

    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = admit(&mut runtime, &frame, &arena, &draw).unwrap();
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted.signature(), signature());
    assert_eq!(
        arena.retire().unwrap_err(),
        ProgramError::Lease(LeaseError::Busy),
        "the unsubmitted frame holds the arena"
    );
    let mut pass = surface.open(&mut frame);
    admitted.record(&mut pass);
    drop(pass);
    let readback = surface.copy_color(&gpu.device, &mut frame);
    assert_eq!(gpu.authority.submitted_serial(), before);
    frame.finish().submit().unwrap();
    assert_eq!(gpu.authority.submitted_serial(), before + 1);

    let bytes = read(&gpu.device, &readback);
    assert_eq!(pixel(&bytes, 32, 32), [0, 255, 0, 255]);
    arena
        .retire()
        .expect("the completed frame released the arena");
    assert_eq!(
        arena.retire().unwrap_err(),
        ProgramError::Lease(LeaseError::Expired)
    );
}

#[test]
fn a_cancelled_frame_releases_the_arena_without_submitting() {
    let gpu = Gpu::new();
    let (mut runtime, mut arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let before = gpu.authority.submitted_serial();
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = admit(&mut runtime, &frame, &arena, &draw).unwrap();
    admitted.retain(&frame).expect("retaining again is allowed");
    frame.finish().cancel();
    assert_eq!(gpu.authority.submitted_serial(), before);
    arena.retire().expect("a cancelled frame holds nothing");
}

#[test]
fn a_frame_from_another_authority_is_refused_by_name() {
    let gpu = Gpu::new();
    let other = Gpu::new();
    let (mut runtime, arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let frame = other.authority.begin(&other.device).unwrap();
    assert_eq!(
        admit(&mut runtime, &frame, &arena, &draw).unwrap_err(),
        ProgramError::Lease(LeaseError::ForeignAuthority)
    );
}

#[test]
fn a_retired_arena_is_refused_by_name_and_expires_what_was_admitted() {
    let gpu = Gpu::new();
    let (mut runtime, mut arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = admit(&mut runtime, &frame, &arena, &draw).unwrap();
    frame.finish().cancel();
    arena.retire().unwrap();

    let frame = gpu.authority.begin(&gpu.device).unwrap();
    assert_eq!(
        admit(&mut runtime, &frame, &arena, &draw).unwrap_err(),
        ProgramError::Lease(LeaseError::Expired)
    );
    assert_eq!(
        admitted.retain(&frame).unwrap_err(),
        ProgramError::Lease(LeaseError::Expired)
    );
}

#[test]
fn a_dropped_arena_expires_what_was_admitted() {
    let gpu = Gpu::new();
    let (mut runtime, arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = admit(&mut runtime, &frame, &arena, &draw).unwrap();
    frame.finish().cancel();
    drop(arena);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    assert_eq!(
        admitted.retain(&frame).unwrap_err(),
        ProgramError::Lease(LeaseError::Expired)
    );
}

#[test]
fn an_arena_smaller_than_the_runtimes_budget_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(ARENA));
    assert_eq!(
        open(&gpu, &mut runtime, ARENA / 2).err().unwrap(),
        ProgramError::ArenaTooSmall {
            budget: ARENA,
            arena: ARENA / 2
        }
    );
    // An arena at least as large as the budget covers everything the runtime can admit.
    open(&gpu, &mut runtime, ARENA).unwrap();
    open(&gpu, &mut runtime, 2 * ARENA).unwrap();
}

#[test]
fn the_runtime_cannot_hold_more_than_its_arena() {
    let gpu = Gpu::new();
    let (mut runtime, _arena) = arena_runtime(&gpu);
    runtime.create_buffer("held", &[0; 16]).unwrap();
    let used = runtime.live_bytes();
    assert_eq!(used, 16);
    let refused = runtime
        .create_buffer("too much", &vec![0; usize::try_from(ARENA).unwrap()])
        .unwrap_err();
    assert_eq!(
        refused,
        ProgramError::OverBudget {
            required: used + ARENA,
            budget: ARENA
        }
    );
    assert_eq!(runtime.live_bytes(), used);
}

#[test]
fn an_arena_belongs_to_the_runtime_that_opened_it() {
    let gpu = Gpu::new();
    let (_opener, arena) = arena_runtime(&gpu);
    let mut stranger = gpu.runtime_with(AllocationBudget(ARENA));
    let draw = draw(&mut stranger);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    assert_eq!(
        admit(&mut stranger, &frame, &arena, &draw).unwrap_err(),
        ProgramError::ForeignArena
    );
}

#[test]
fn a_pass_with_other_attachments_than_the_admitted_draws_is_a_named_mismatch() {
    let gpu = Gpu::new();
    let (mut runtime, arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = admit(&mut runtime, &frame, &arena, &draw).unwrap();
    assert_eq!(admitted.check_target(signature()), Ok(()));
    let other = TargetSignature::new(&[wgpu::TextureFormat::Rgba16Float], Some(DEPTH), 1).unwrap();
    assert_eq!(
        admitted.check_target(other),
        Err(ProgramError::TargetMismatch {
            prepared: signature(),
            pass: other
        })
    );
}

#[test]
fn an_encoder_the_authority_does_not_own_is_refused_before_anything_is_prepared() {
    let gpu = Gpu::new();
    let (mut runtime, arena) = arena_runtime(&gpu);
    let draw = draw(&mut runtime);
    let foreign = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(matches!(
        admit(&mut runtime, &foreign, &arena, &draw).unwrap_err(),
        ProgramError::Transaction(_)
    ));
    let pipelines = runtime.cached_pipelines();
    assert_eq!(
        pipelines, 0,
        "no pipeline was built for a refused admission"
    );
}
