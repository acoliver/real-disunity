//! The preparation of the temporal mask: a 3x3 maximum per channel into a per-slot prepared mask.
//! The tests run the "temporal mask dilate" stage of a real slot's stage set on lavapipe or
//! another device, over mask data written into the frame targets.
use crate::hdr_post::HdrPost;
use crate::hdr_temporal_build_tests as build;
use crate::{FrameTargets, Operation, StageSet, StageTarget, TEMPORAL_MASK_FORMAT};

const RENDER: [u32; 2] = [10, 6];
const OUTPUT: [u32; 2] = [16, 12];
const STAGE: &str = "temporal mask dilate";

fn width() -> usize {
    RENDER[0] as usize
}

fn index(x: usize, y: usize) -> usize {
    y * width() + x
}

/// The expected maximum per channel over the in-bounds 3x3 neighbourhood of every texel.
fn reference(mask: &[[u8; 2]]) -> Vec<[u8; 2]> {
    let height = RENDER[1] as usize;
    let mut out = vec![[0; 2]; mask.len()];
    for y in 0..height {
        for x in 0..width() {
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width() - 1) {
                    for c in 0..2 {
                        out[index(x, y)][c] = out[index(x, y)][c].max(mask[index(nx, ny)][c]);
                    }
                }
            }
        }
    }
    out
}

fn upload(queue: &wgpu::Queue, texture: &wgpu::Texture, mask: &[[u8; 2]]) {
    let bytes: Vec<u8> = mask.iter().flatten().copied().collect();
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(RENDER[0] * 2),
            rows_per_image: Some(RENDER[1]),
        },
        crate::targets::extent(RENDER),
    );
}

struct Slot {
    targets: FrameTargets,
    set: StageSet,
}

impl Slot {
    fn new(device: &wgpu::Device) -> Self {
        let targets = build::masked_targets(device, RENDER, OUTPUT);
        let post = HdrPost::new(device, &build::settings()).unwrap();
        let set = build::slots(&post, device, &targets).remove(0);
        Self { targets, set }
    }

    /// The prepared mask: the only `Rg8Unorm` texture a slot owns.
    fn prepared(&self) -> &wgpu::Texture {
        let mut found = self
            .set
            .resources
            .textures
            .iter()
            .filter(|texture| texture.format() == TEMPORAL_MASK_FORMAT);
        let prepared = found
            .next()
            .expect("a slot with a mask owns a prepared mask");
        assert!(found.next().is_none());
        assert_eq!([prepared.width(), prepared.height()], RENDER);
        prepared
    }

    /// Dilate `mask` and read the prepared mask back.
    fn prepare(&self, gpu: &(wgpu::Device, wgpu::Queue), mask: &[[u8; 2]]) -> Vec<[u8; 2]> {
        let (device, queue) = gpu;
        upload(queue, self.targets.temporal_mask_texture().unwrap(), mask);
        let stage = self
            .set
            .stages
            .iter()
            .find(|stage| stage.id.name == STAGE)
            .expect("the preparation stage is scheduled when the mask exists");
        let Operation::Fullscreen {
            pipeline,
            groups,
            target: StageTarget::Texture(view),
        } = &stage.operation
        else {
            panic!("the preparation is a fullscreen pass into one texture");
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(STAGE),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &groups[0], &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        crate::temporal_mask_clear_tests::read_mask((device, queue), self.prepared())
    }
}

fn pattern(texels: &[(usize, usize, [u8; 2])]) -> Vec<[u8; 2]> {
    let mut mask = vec![[0; 2]; width() * RENDER[1] as usize];
    for &(x, y, value) in texels {
        mask[index(x, y)] = value;
    }
    mask
}

#[test]
fn a_single_texel_grows_to_its_3x3_neighbourhood_and_no_further() {
    let gpu = crate::gpu_tests::gpu();
    let slot = Slot::new(&gpu.0);
    let prepared = slot.prepare(&gpu, &pattern(&[(4, 3, [255, 255])]));
    for y in 0..RENDER[1] as usize {
        for x in 0..width() {
            let inside = x.abs_diff(4) <= 1 && y.abs_diff(3) <= 1;
            let expected = if inside { [255, 255] } else { [0, 0] };
            assert_eq!(prepared[index(x, y)], expected, "texel ({x}, {y})");
        }
    }
}

#[test]
fn corners_and_edges_dilate_over_the_texels_inside_the_image() {
    let gpu = crate::gpu_tests::gpu();
    let slot = Slot::new(&gpu.0);
    let last = (width() - 1, RENDER[1] as usize - 1);
    let seeds = [
        (0, 0),
        (last.0, 0),
        (0, last.1),
        last,
        (0, 3),
        (last.0, 2),
        (5, 0),
        (4, last.1),
    ];
    for (x, y) in seeds {
        let mask = pattern(&[(x, y, [255, 255])]);
        assert_eq!(
            slot.prepare(&gpu, &mask),
            reference(&mask),
            "seed ({x}, {y})"
        );
    }
}

#[test]
fn channels_dilate_independently_and_keep_the_largest_value_in_reach() {
    let gpu = crate::gpu_tests::gpu();
    let slot = Slot::new(&gpu.0);
    let mask = pattern(&[
        (2, 2, [200, 0]),
        (3, 2, [90, 0]),
        (4, 3, [0, 128]),
        (8, 5, [10, 20]),
    ]);
    let prepared = slot.prepare(&gpu, &mask);
    assert_eq!(prepared, reference(&mask));
    assert_eq!(prepared[index(3, 2)], [200, 128], "both producers reach it");
}

#[test]
fn a_half_strength_value_passes_through_within_one_quantisation_step() {
    let gpu = crate::gpu_tests::gpu();
    let slot = Slot::new(&gpu.0);
    let half: u8 = 128;
    let mask = vec![[half, half]; width() * RENDER[1] as usize];
    for texel in slot.prepare(&gpu, &mask) {
        assert!(
            texel.iter().all(|&byte| byte.abs_diff(half) <= 1),
            "{texel:?}"
        );
    }
}

#[test]
fn an_all_zero_mask_prepares_to_zero() {
    let gpu = crate::gpu_tests::gpu();
    let slot = Slot::new(&gpu.0);
    let zero = pattern(&[]);
    assert_eq!(slot.prepare(&gpu, &zero), zero);
}
