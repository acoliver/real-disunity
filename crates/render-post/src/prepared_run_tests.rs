//! Compute work staged one after another is recorded into one compute pass, in staging order: each
//! dispatch reads what the one before it wrote, a transfer between two dispatches sees the first
//! and not the second, and a refusal inside the run names the refusing commands.

use crate::prepared_test_support::{BufferCopy, PassDispatch, Refused, pipeline};
use crate::{FrameError, PrepareContext, PreparedAccess};

/// One step of the chain: reads the value and the texel the step before wrote, writes the next
/// value and its texel into the other image.
const STEP: &str = "
@group(0) @binding(0) var source: texture_2d<u32>;
@group(0) @binding(1) var destination: texture_storage_2d<r32uint, write>;
@group(0) @binding(2) var<storage, read_write> values: array<u32, 2>;
@compute @workgroup_size(1) fn main() {
    let next = values[0] * 31u + textureLoad(source, vec2<i32>(0, 0), 0).x + 7u;
    values[0] = next;
    values[1] = values[1] + 1u;
    textureStore(destination, vec2<i32>(0, 0), vec4<u32>(next ^ 0x5a5au, 0u, 0u, 0u));
}";

/// The value and the step count the chain starts from; both images start at zero.
const START: [u32; 2] = [1, 0];

/// The values and the two images' texels after `steps` steps, step `k` reading image `k % 2`.
fn replay(steps: usize) -> ([u32; 2], [u32; 2]) {
    let (mut values, mut images) = (START, [0_u32; 2]);
    for step in 0..steps {
        let source = step % 2;
        let next = values[0]
            .wrapping_mul(31)
            .wrapping_add(images[source])
            .wrapping_add(7);
        values = [next, values[1] + 1];
        images[1 - source] = next ^ 0x5a5a;
    }
    (values, images)
}

/// Two one-texel `r32uint` images, a value buffer, and the step's pipeline with a bind group for
/// each direction: `groups[k]` reads image `k` and writes the other.
struct Chain {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    groups: [wgpu::BindGroup; 2],
    images: [wgpu::Texture; 2],
    values: wgpu::Buffer,
}

impl Chain {
    fn new() -> Self {
        let (device, queue) = crate::gpu_tests::gpu();
        let pipeline = pipeline(&device, "chain step", STEP);
        let image = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let images = [image("chain image 0"), image("chain image 1")];
        let values = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chain values"),
            size: 8,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let start: Vec<u8> = START.iter().flat_map(|word| word.to_le_bytes()).collect();
        queue.write_buffer(&values, 0, &start);
        let group = |source: usize| {
            let view =
                |image: &wgpu::Texture| image.create_view(&wgpu::TextureViewDescriptor::default());
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("chain step"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view(&images[source])),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view(&images[1 - source])),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: values.as_entire_binding(),
                    },
                ],
            })
        };
        let groups = [group(0), group(1)];
        Self {
            device,
            queue,
            pipeline,
            groups,
            images,
            values,
        }
    }

    /// Step `step` as prepared commands.
    fn prepared(&self, step: usize) -> PassDispatch {
        PassDispatch::new(&self.pipeline, &self.groups[step % 2])
    }

    /// Stages step `step`: through [`PrepareContext::dispatch`] when `context_dispatch`, else as
    /// prepared commands.
    fn stage(
        &self,
        context: &mut PrepareContext<'_>,
        step: usize,
        context_dispatch: bool,
    ) -> Result<(), FrameError> {
        if context_dispatch {
            context.dispatch("step", &self.pipeline, &self.groups[step % 2], [1, 1, 1]);
            Ok(())
        } else {
            context.record_prepared(
                "step",
                PreparedAccess::default(),
                Box::new(self.prepared(step)),
            )
        }
    }

    fn readback(&self, label: &str, size: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Records copies of the values and of both images' texels into readbacks.
    fn read_all(&self, encoder: &mut wgpu::CommandEncoder) -> [wgpu::Buffer; 3] {
        let values = self.readback("values readback", 8);
        encoder.copy_buffer_to_buffer(&self.values, 0, &values, 0, 8);
        let texel = |image: &wgpu::Texture, encoder: &mut wgpu::CommandEncoder| {
            let buffer = self.readback("texel readback", 4);
            encoder.copy_texture_to_buffer(
                image.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout::default(),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            buffer
        };
        let first = texel(&self.images[0], encoder);
        let second = texel(&self.images[1], encoder);
        [values, first, second]
    }

    /// Prepares and submits one frame with `record`, then the words of each buffer it returned.
    fn run(
        &self,
        record: impl FnOnce(&mut wgpu::CommandEncoder) -> Result<Vec<wgpu::Buffer>, FrameError>,
    ) -> Vec<Vec<u32>> {
        let mut read = Vec::new();
        let prepared = crate::test_authority(&self.queue)
            .prepare(&self.device, |encoder, _| {
                read = record(encoder)?;
                Ok::<_, FrameError>(())
            })
            .unwrap();
        prepared.submit().unwrap();
        read.iter().map(|buffer| self.words(buffer)).collect()
    }

    fn words(&self, buffer: &wgpu::Buffer) -> Vec<u32> {
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        receiver.recv().unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        let words = mapped
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
            .collect();
        drop(mapped);
        buffer.unmap();
        words
    }
}

/// Stages `units` on a preparation of `encoder` and finishes it.
fn prepare(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    units: impl FnOnce(&mut PrepareContext<'_>) -> Result<(), FrameError>,
) -> Result<(), FrameError> {
    let mut context = PrepareContext::new(device, encoder)?;
    context.open_preparation();
    units(&mut context)?;
    context.finish_preparation()?;
    Ok(())
}

#[test]
fn a_chain_of_dispatches_in_one_run_reads_what_each_wrote() {
    const STEPS: usize = 33;
    let chain = Chain::new();
    let read = chain.run(|encoder| {
        prepare(&chain.device, encoder, |context| {
            for step in 0..STEPS {
                chain.stage(context, step, step % 3 == 2)?;
            }
            Ok(())
        })?;
        Ok(chain.read_all(encoder).into())
    });
    let (values, images) = replay(STEPS);
    assert_eq!(read[0], values, "values after {STEPS} steps");
    assert_eq!(
        [read[1][0], read[2][0]],
        images,
        "texels after {STEPS} steps"
    );
}

#[test]
fn a_transfer_between_dispatches_sees_the_first_and_not_the_second() {
    let chain = Chain::new();
    let snapshots = [
        chain.readback("snapshot 1", 8),
        chain.readback("snapshot 2", 8),
    ];
    let read = chain.run(|encoder| {
        prepare(&chain.device, encoder, |context| {
            for (step, snapshot) in snapshots.iter().enumerate() {
                chain.stage(context, step, false)?;
                let copy = BufferCopy {
                    source: chain.values.clone(),
                    destination: snapshot.clone(),
                    bytes: 8,
                };
                context.record_prepared("snapshot", PreparedAccess::default(), Box::new(copy))?;
            }
            Ok(())
        })?;
        Ok(snapshots.to_vec())
    });
    assert_eq!(
        read[0],
        replay(1).0,
        "the transfer after the first dispatch"
    );
    assert_eq!(
        read[1],
        replay(2).0,
        "the transfer after the second dispatch"
    );
}

#[test]
fn a_refusal_inside_a_run_names_the_refusing_commands() {
    let chain = Chain::new();
    let refused = crate::test_authority(&chain.queue)
        .prepare(&chain.device, |encoder, _| {
            prepare(&chain.device, encoder, |context| {
                chain.stage(context, 0, false)?;
                let mut refusing = chain.prepared(1);
                refusing.refuse = true;
                context.record_prepared(
                    "refusing",
                    PreparedAccess::default(),
                    Box::new(refusing),
                )?;
                context.record_prepared(
                    "after",
                    PreparedAccess::default(),
                    Box::new(chain.prepared(0)),
                )
            })
        })
        .expect_err("the frame was prepared");
    let worldrunner_render_gpu::submission::PrepareError::Preparation(error) = refused else {
        panic!("expected the preparation's refusal, found {refused:?}");
    };
    assert!(
        matches!(
            &error,
            FrameError::PreparedRefused { label: "refusing", refusal }
                if refusal.downcast_ref::<Refused>() == Some(&Refused)
        ),
        "{error:?}"
    );
}
