//! Reset equivalence of the temporal bank, built through the public `ToneMapOutput`.
//! The first output after any reset cause must equal the first output of a fresh chain on the
//! same device, and the slot schedule must not change any output. Both comparisons run the
//! same code twice on one device, so they are byte comparisons.
use crate::hdr_temporal_build_tests::{public_factory, settings};
use crate::taa_gpu_test_support::{
    SHADER, f16_bits, noise, passthrough_pipeline, sampled, view, write_texture,
};
use crate::{FrameConfig, PostChain, Resources, TaaSettings, TemporalBlock, ToneMapOutput};

const JITTER: [[f32; 2]; 4] = [
    [-0.375, -0.4],
    [-0.125, -0.275],
    [0.125, -0.15],
    [0.375, -0.025],
];

/// Deterministic noise in half floats, opaque, different for every seed.
fn scene_bytes([width, height]: [u32; 2], seed: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for row in 0..height {
        for column in 0..width {
            for channel in 0..4 {
                let (cx, cy) = (i32::try_from(column).unwrap(), i32::try_from(row).unwrap());
                let value = if channel == 3 {
                    1.0
                } else {
                    noise(cx, cy, channel + 4 * seed)
                };
                bytes.extend(f16_bits(value).to_le_bytes());
            }
        }
    }
    bytes
}

struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    chain: PostChain<ToneMapOutput>,
    output: [u32; 2],
}

impl Rig {
    fn new(gpu: &(wgpu::Device, wgpu::Queue), render: [u32; 2], output: [u32; 2]) -> Self {
        let (device, queue) = gpu.clone();
        let config = FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent(render);
        let factory = public_factory(&device, &settings());
        let chain = PostChain::new(&device, config, Resources::default(), factory).unwrap();
        Self {
            device,
            queue,
            chain,
            output,
        }
    }

    fn render(&self) -> [u32; 2] {
        self.chain.targets().config().size
    }

    fn block(&self, frame: u32, history_valid: bool) -> TemporalBlock {
        TemporalBlock {
            render: self.render(),
            output: self.output,
            jitter: JITTER[usize::try_from(frame).unwrap() % JITTER.len()],
            phase_count: 8,
            settings: TaaSettings::new(0.9),
            history_valid,
        }
    }

    /// The resolved target is not a copy destination, so the scene goes through an identity pass.
    fn upload_scene(&self, encoder: &mut wgpu::CommandEncoder, seed: u32) {
        let [w, h] = self.render();
        let bytes = scene_bytes([w, h], seed);
        let source = sampled(
            &self.device,
            "scene source",
            [w, h],
            wgpu::TextureFormat::Rgba16Float,
        );
        write_texture(&self.queue, &source, &bytes, 8);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("scene upload"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
        let pipeline = passthrough_pipeline(
            &self.device,
            &module,
            "identity",
            Some(wgpu::TextureFormat::Rgba16Float),
        );
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view(&source)),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene upload"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.chain.targets().resolved(),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Runs one frame on `slot` and returns the tone-mapped output bytes.
    fn frame(&mut self, seed: u32, block: &TemporalBlock, slot: usize) -> Vec<u8> {
        self.chain.select_frame_slot(slot);
        let out = crate::color_texture(
            &self.device,
            "out",
            self.output,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.upload_scene(&mut encoder, seed);
        let factory = self.chain.factory();
        factory.prepare_temporal(&mut encoder, slot, block).unwrap();
        factory
            .prepare_effects(&mut encoder, slot, 0.031, true)
            .unwrap();
        let view = out.create_view(&wgpu::TextureViewDescriptor::default());
        self.chain.encode(&mut encoder, &view);
        self.queue.submit([encoder.finish()]);
        crate::Readback::new(
            &self.device,
            FrameConfig::new(self.output, out.format(), 1),
            1_000_000,
        )
        .unwrap()
        .read(&self.device, &crate::test_authority(&self.queue), &out)
        .unwrap()
    }

    /// Three frames of history: the first resets, the next two reproject.
    fn warm(&mut self) {
        for frame in 0..3 {
            let block = self.block(frame, frame > 0);
            self.frame(frame + 1, &block, 0);
        }
    }

    fn first_output_after_reset(&mut self) -> Vec<u8> {
        let block = self.block(5, false);
        self.frame(9, &block, 0)
    }
}

const RENDER: [u32; 2] = [10, 10];
const OUTPUT: [u32; 2] = [16, 16];

fn assert_reset_matches_fresh(
    gpu: &(wgpu::Device, wgpu::Queue),
    warmed: &mut Rig,
    render: [u32; 2],
    output: [u32; 2],
) {
    let after = warmed.first_output_after_reset();
    let fresh = Rig::new(gpu, render, output).first_output_after_reset();
    assert_eq!(after, fresh, "reset output differs from a fresh chain");
}

#[test]
fn history_changes_the_output_so_the_reset_comparisons_are_not_vacuous() {
    let gpu = crate::gpu_tests::gpu();
    let mut warmed = Rig::new(&gpu, RENDER, OUTPUT);
    warmed.warm();
    let block = warmed.block(5, true);
    let reprojected = warmed.frame(9, &block, 0);
    let fresh = Rig::new(&gpu, RENDER, OUTPUT).first_output_after_reset();
    assert_ne!(reprojected, fresh);
}

#[test]
fn a_reset_in_place_matches_a_fresh_chain() {
    let gpu = crate::gpu_tests::gpu();
    let mut warmed = Rig::new(&gpu, RENDER, OUTPUT);
    warmed.warm();
    assert_reset_matches_fresh(&gpu, &mut warmed, RENDER, OUTPUT);
}

#[test]
fn a_reset_after_a_render_only_resize_matches_a_fresh_chain() {
    let gpu = crate::gpu_tests::gpu();
    let mut warmed = Rig::new(&gpu, RENDER, OUTPUT);
    warmed.warm();
    warmed
        .chain
        .resize_extents(&warmed.device, [12, 12], OUTPUT)
        .unwrap();
    assert_reset_matches_fresh(&gpu, &mut warmed, [12, 12], OUTPUT);
}

#[test]
fn a_reset_after_an_output_only_resize_matches_a_fresh_chain() {
    let gpu = crate::gpu_tests::gpu();
    let mut warmed = Rig::new(&gpu, RENDER, OUTPUT);
    warmed.warm();
    warmed
        .chain
        .resize_extents(&warmed.device, RENDER, [18, 18])
        .unwrap();
    warmed.output = [18, 18];
    assert_reset_matches_fresh(&gpu, &mut warmed, RENDER, [18, 18]);
}

#[test]
fn a_reset_after_resizing_both_extents_matches_a_fresh_chain() {
    let gpu = crate::gpu_tests::gpu();
    let mut warmed = Rig::new(&gpu, RENDER, OUTPUT);
    warmed.warm();
    warmed
        .chain
        .resize_extents(&warmed.device, [14, 14], [20, 20])
        .unwrap();
    warmed.output = [20, 20];
    assert_reset_matches_fresh(&gpu, &mut warmed, [14, 14], [20, 20]);
}

fn run_schedule(gpu: &(wgpu::Device, wgpu::Queue), slots: [usize; 4]) -> Vec<Vec<u8>> {
    let mut rig = Rig::new(gpu, RENDER, OUTPUT);
    (0..4)
        .map(|frame| {
            let block = rig.block(frame, frame > 0);
            rig.frame(frame + 1, &block, slots[usize::try_from(frame).unwrap()])
        })
        .collect()
}

#[test]
fn the_slot_schedule_does_not_change_any_output() {
    let gpu = crate::gpu_tests::gpu();
    let repeated = run_schedule(&gpu, [0, 0, 0, 0]);
    let rotating = run_schedule(&gpu, [0, 1, 2, 0]);
    for (frame, (a, b)) in repeated.iter().zip(&rotating).enumerate() {
        assert_eq!(a, b, "frame {frame} differs between slot schedules");
    }
}
