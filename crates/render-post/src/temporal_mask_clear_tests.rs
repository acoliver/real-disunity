//! The per-frame clear of the temporal mask, and a frame cancelled after its producers ran.
use crate::{FrameConfig, FrameTargets, TargetOptions};

const RENDER: [u32; 2] = [10, 6];
const OUTPUT: [u32; 2] = [16, 12];

fn targets(device: &wgpu::Device) -> FrameTargets {
    let config =
        FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(RENDER);
    let options = TargetOptions {
        velocity: false,
        temporal_mask: true,
    };
    FrameTargets::with_options(device, config, options).unwrap()
}

/// The `Rg8Unorm` bytes of a mask, row major.
pub(crate) fn read_mask(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    texture: &wgpu::Texture,
) -> Vec<[u8; 2]> {
    let (width, height) = (texture.width(), texture.height());
    let pitch = (width * 2).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mask readback"),
        size: u64::from(pitch * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(height),
            },
        },
        crate::targets::extent([width, height]),
    );
    let submission = queue.submit([encoder.finish()]);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    let row = usize::try_from(width).unwrap() * 2;
    let pitch = usize::try_from(pitch).unwrap();
    data.chunks_exact(pitch)
        .flat_map(|line| line[..row].chunks_exact(2))
        .map(|texel| [texel[0], texel[1]])
        .collect()
}

/// A producer pass that writes `value` into both channels, as a feature drawing into the mask.
pub(crate) fn write_mask(encoder: &mut wgpu::CommandEncoder, targets: &FrameTargets, value: f64) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("test mask producer"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: targets.temporal_mask().unwrap(),
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: value,
                    g: value,
                    b: 0.0,
                    a: 0.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}

#[test]
fn the_clear_resets_what_an_earlier_frame_left_in_the_mask() {
    let gpu = crate::gpu_tests::gpu();
    let targets = targets(&gpu.0);
    let texture = targets.temporal_mask_texture().unwrap();
    let mut producer = gpu
        .0
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    write_mask(&mut producer, &targets, 1.0);
    gpu.1.submit([producer.finish()]);
    let written = read_mask((&gpu.0, &gpu.1), texture);
    assert!(written.iter().all(|&texel| texel == [255, 255]));

    let mut next = gpu
        .0
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    targets.encode_mask_clear(&mut next);
    gpu.1.submit([next.finish()]);
    let cleared = read_mask((&gpu.0, &gpu.1), texture);
    assert_eq!(cleared.len(), 60);
    assert!(cleared.iter().all(|&texel| texel == [0, 0]));
}

#[test]
fn clearing_targets_without_a_mask_encodes_nothing() {
    let gpu = crate::gpu_tests::gpu();
    let config =
        FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(RENDER);
    let plain = FrameTargets::new(&gpu.0, config).unwrap();
    let mut encoder = gpu
        .0
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    plain.encode_mask_clear(&mut encoder);
    // An encoder with no pass finishes to a buffer that submits and completes.
    gpu.1.submit([encoder.finish()]);
}

mod cancelled {
    use crate::temporal_disocclusion_scene as scene;
    use crate::temporal_scene_test_rig::SceneRig;

    const SCENE_RENDER: [u32; 2] = [80, 48];
    const WARM: u64 = 8;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the bar's left edge and velocity are small canvas coordinates"
    )]
    fn bar(index: u64) -> [f32; 4] {
        [scene::bar_left(index) as f32, scene::SPEED as f32, 0.0, 0.0]
    }

    fn frame(rig: &SceneRig, index: u64) {
        let camera = crate::taa_reconstruct_test_rig::static_camera();
        rig.frame(index, true, (bar(index), Some(camera)));
    }

    fn warmed(gpu: &(wgpu::Device, wgpu::Queue)) -> SceneRig {
        let rig = SceneRig::new(gpu, SCENE_RENDER, scene::wgsl());
        for index in 0..WARM {
            rig.frame(
                index,
                index > 0,
                (
                    bar(index),
                    Some(crate::taa_reconstruct_test_rig::static_camera()),
                ),
            );
        }
        rig
    }

    fn bits(image: Vec<f64>) -> Vec<u64> {
        image.into_iter().map(f64::to_bits).collect()
    }

    /// A frame whose producers wrote the mask and whose remaining work was cancelled leaves
    /// nothing that the next frame reads: the next frame equals a renderer that never saw it.
    /// Both renderers run the same code on one device, so the comparison is exact.
    #[test]
    fn a_mask_written_in_a_cancelled_frame_cannot_affect_the_next_frame() {
        let gpu = crate::gpu_tests::gpu();
        let cancelled = warmed(&gpu);
        cancelled.abandon_mask_write(1.0);
        cancelled.cancelled_frame(WARM, true, bar(WARM));
        frame(&cancelled, WARM);
        let fresh = warmed(&gpu);
        frame(&fresh, WARM);
        assert_eq!(bits(cancelled.image()), bits(fresh.image()));
        assert!(cancelled.mask().iter().all(|&texel| texel == [0, 0]));
    }
}
