//! Prepared packets in a caller's frame and in a renderer's pass: admission against an arena,
//! the frame's authority, and how long the packed constants stay counted.

use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_gpu::submission::LeaseError;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime, TargetSignature};

use crate::common::{
    COLOR, DEPTH, Gpu, RGBA8, Rig, assert_all, colored, draw_of, full_target_triangle, rectangle,
    tint_program,
};

const ARENA: u64 = 1 << 16;
const SIZE: u32 = 64;
const ROW: u32 = SIZE * 4;

fn signature() -> TargetSignature {
    TargetSignature::new(&[RGBA8], Some(DEPTH), 1).unwrap()
}

/// A runtime whose budget the renderer's configuration reserved as an arena, before any
/// allocation.
fn arena_runtime(gpu: &Gpu) -> (ProgramRuntime, ProgramArena) {
    let runtime = gpu.runtime_with(AllocationBudget(ARENA));
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
        .reserve::<()>(&limits, ProgramRuntime::arena_allocations(ARENA))
        .unwrap();
    let arena = runtime.open_arena(reservation).unwrap();
    (runtime, arena)
}

/// A color and depth attachment pair the test owns, for a pass it opens itself.
struct Surface {
    color: wgpu::Texture,
    depth: wgpu::Texture,
}

impl Surface {
    fn new(device: &wgpu::Device) -> Self {
        let make = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("join surface"),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: usage | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        Self {
            color: make(RGBA8, wgpu::TextureUsages::COPY_SRC),
            depth: make(DEPTH, wgpu::TextureUsages::empty()),
        }
    }

    fn open<'e>(&self, encoder: &'e mut wgpu::CommandEncoder) -> wgpu::RenderPass<'e> {
        let color = self
            .color
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("join pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    fn copy_color(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::Buffer {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("join readback"),
            size: u64::from(ROW * SIZE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            self.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW),
                    rows_per_image: Some(SIZE),
                },
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
        );
        buffer
    }
}

fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    buffer.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.expect("the readback maps");
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .expect("the queue drains");
    let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    bytes
}

fn center(bytes: &[u8]) -> [u8; 4] {
    let at = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

#[test]
fn admitted_material_draws_record_into_the_callers_pass_and_hold_the_arena_until_it_completes() {
    let gpu = Gpu::new();
    let (mut runtime, mut arena) = arena_runtime(&gpu);
    let program = tint_program(&mut runtime);
    let material = runtime.create_material(program, colored(COLOR)).unwrap();
    let quad = rectangle(&mut runtime, [-0.5, -0.5, 0.5, 0.5], 0.5);
    let draw = draw_of(material, quad, 6);
    let surface = Surface::new(&gpu.device);
    let before = gpu.authority.submitted_serial();

    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    let packet = runtime.prepare_material_draw(&frame, &draw).unwrap();
    // Editing or releasing the material after preparation does not touch the packet.
    assert!(runtime.release_material(material));
    let admitted = runtime
        .admit_material_draws(&frame, &arena, signature(), &[packet])
        .unwrap();
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
    let want = COLOR.map(|value| (value * 255.0).round() as u8);
    for (got, want) in center(&bytes).into_iter().zip(want) {
        assert!(got.abs_diff(want) <= 1, "got {got}, want {want}");
    }
    arena
        .retire()
        .expect("the completed frame released the arena");
}

#[test]
fn a_frame_from_another_authority_or_a_bare_encoder_is_refused_by_name() {
    let gpu = Gpu::new();
    let other = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);

    let foreign = other.authority.begin(&other.device).unwrap();
    assert_eq!(
        rig.runtime
            .prepare_material_draw(&foreign, &draw)
            .unwrap_err(),
        ProgramError::Lease(LeaseError::ForeignAuthority)
    );
    foreign.finish().cancel();

    let bare = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(matches!(
        rig.runtime.prepare_material_draw(&bare, &draw).unwrap_err(),
        ProgramError::Transaction(_)
    ));
}

#[test]
fn packed_constants_stay_counted_until_the_frame_that_prepared_them_is_cancelled() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);
    let before = rig.runtime.live_bytes();

    let frame = gpu.authority.begin(&gpu.device).unwrap();
    let packet = rig.runtime.prepare_material_draw(&frame, &draw).unwrap();
    assert!(rig.runtime.live_bytes() > before, "the snapshot is counted");
    drop(packet);
    assert!(
        rig.runtime.live_bytes() > before,
        "the frame still holds the snapshot after the packet is dropped"
    );
    frame.finish().cancel();
    assert_eq!(rig.runtime.live_bytes(), before);

    rig.render(&draw).unwrap();
    assert_all(&rig.read().unwrap(), COLOR);
}
