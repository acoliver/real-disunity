use super::{DepthRange, ReductionTicket, SdsmError};
use crate::frame::metadata::{FrameProduct, MetadataMeaning, MetadataSampling};
use worldrunner_render_gpu::submission::{Publications, TransactionStatus, capture::WriteKey};
use worldrunner_render_post::AttachmentView;

/// Compute resources bind only the shared metre-depth and coverage products.
#[derive(Debug)]
pub struct DepthReducer {
    initialize: wgpu::ComputePipeline,
    reduce: wgpu::ComputePipeline,
    group: wgpu::BindGroup,
    result: wgpu::Buffer,
    size: [u32; 2],
    timing: Option<worldrunner_render_post::TimingState>,
}
impl DepthReducer {
    pub(super) fn set_timing(&mut self, timing: worldrunner_render_post::TimingState) {
        self.timing = Some(timing);
    }

    pub fn write_key(&self) -> WriteKey {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.result.hash(&mut hash);
        WriteKey::Readback(hash.finish())
    }

    pub fn new(
        device: &wgpu::Device,
        depth: (FrameProduct, &AttachmentView),
        coverage: (FrameProduct, &AttachmentView),
    ) -> Result<Self, SdsmError> {
        validate_sources(depth, coverage)?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("receiver depth statistics"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../sdsm_reduce.wgsl").into()),
        });
        let layout = reduction_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("receiver depth statistics"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("receiver depth statistics"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let result = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("receiver depth statistics"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("receiver depth statistics"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: result.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth.1.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(coverage.1.view()),
                },
            ],
        });
        Ok(Self {
            initialize: pipeline("initialize"),
            reduce: pipeline("reduce"),
            group,
            result,
            size: depth.1.size(),
            timing: None,
        })
    }
    /// Allocation-local buffers prevent mapped-slot reuse and copy/map races.
    /// Publication is tied to the transaction; cancelled prefixes cannot be read.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        ticket: ReductionTicket,
        key: WriteKey,
    ) -> Result<PendingReduction, SdsmError> {
        ticket.validate()?;
        let transaction = Publications::for_encoder(encoder)?;
        transaction.write(key)?;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("receiver statistics readback"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        for (name, pipeline, groups) in [
            ("sdsm receiver initialize", &self.initialize, [1, 1]),
            (
                "sdsm receiver reduce",
                &self.reduce,
                [self.size[0].div_ceil(8), self.size[1].div_ceil(8)],
            ),
        ] {
            let mut pass = match &self.timing {
                Some(timing) => timing
                    .begin_compute_pass(encoder, worldrunner_render_post::StageId::new(name, 0)),
                None => encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(name),
                    timestamp_writes: None,
                }),
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.dispatch_workgroups(groups[0], groups[1], 1);
        }
        encoder.copy_buffer_to_buffer(&self.result, 0, &buffer, 0, 16);
        Ok(PendingReduction {
            buffer,
            ticket,
            transaction,
            mapping: None,
        })
    }
}
fn reduction_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entries = [
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZeroU64::new(16),
            },
            count: None,
        },
        texture_entry(1),
        texture_entry(2),
    ];
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("receiver depth statistics"),
        entries: &entries,
    })
}

fn validate_sources(
    depth: (FrameProduct, &AttachmentView),
    coverage: (FrameProduct, &AttachmentView),
) -> Result<(), SdsmError> {
    if depth.0 != FrameProduct::linear_depth(0)
        || coverage.0 != FrameProduct::normals()
        || depth.0.meaning() != MetadataMeaning::ConservativeDepthMeters
        || coverage.0.meaning() != MetadataMeaning::ViewNormalCoverage
        || depth.0.sampling() != MetadataSampling::PixelCenter
        || coverage.0.sampling() != MetadataSampling::PixelCenter
        || depth.0.resource() != depth.1.resource()
        || coverage.0.resource() != coverage.1.resource()
        || depth.1.size() != coverage.1.size()
        || depth.1.format() != wgpu::TextureFormat::R32Float
        || coverage.1.format() != wgpu::TextureFormat::Rgba16Float
    {
        return Err(SdsmError::Metadata);
    }
    for (product, attachment) in [depth, coverage] {
        let texture = attachment.texture();
        if texture.sample_count() != product.sampling().samples()
            || !texture
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(SdsmError::Metadata);
        }
    }
    Ok(())
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

#[derive(Debug)]
pub struct PendingReduction {
    buffer: wgpu::Buffer,
    pub ticket: ReductionTicket,
    transaction: Publications,
    mapping: Option<std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
}
impl PendingReduction {
    pub fn begin_mapping(&mut self) -> Result<(), SdsmError> {
        if self.transaction.status() != TransactionStatus::Submitted {
            return Err(SdsmError::NotSubmitted);
        }
        if self.mapping.is_none() {
            let (send, receive) = std::sync::mpsc::channel();
            self.buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = send.send(result);
                });
            self.mapping = Some(receive);
        }
        Ok(())
    }
    /// Nonblocking polling never changes source-frame identity or latency.
    pub fn poll(&mut self, device: &wgpu::Device) -> Result<Option<DepthRange>, SdsmError> {
        self.begin_mapping()?;
        device.poll(wgpu::PollType::Poll)?;
        match self.mapping.as_ref().expect("mapping started").try_recv() {
            Ok(result) => {
                result?;
                self.decode().map(Some)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(SdsmError::Callback),
        }
    }
    pub fn wait(
        mut self,
        device: &wgpu::Device,
        submission: wgpu::SubmissionIndex,
    ) -> Result<DepthRange, SdsmError> {
        self.begin_mapping()?;
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        self.mapping
            .as_ref()
            .expect("mapping started")
            .try_recv()
            .map_err(|_| SdsmError::Callback)??;
        self.decode()
    }
    fn decode(&mut self) -> Result<DepthRange, SdsmError> {
        self.ticket.validate()?;
        let mapped = self.buffer.slice(..).get_mapped_range()?;
        let words: Vec<_> = mapped
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte chunks")))
            .collect();
        let result = DepthRange::decode(&words);
        drop(mapped);
        self.buffer.unmap();
        result.map_err(SdsmError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_failed_mapping_is_typed_error() {
        let context = pollster::block_on(crate::device::GpuContext::request_with_features(
            wgpu::Features::empty(),
        ))
        .unwrap();
        let state = crate::shadows::sdsm::AdoptionState::new(crate::shadows::sdsm::SdsmSettings {
            policy: crate::shadows::sdsm::AdoptionPolicy::SameFrameCapture,
            range: crate::shadows::sdsm::RangeFit {
                near_step: 1.0,
                far_step: 1.0,
                minimum_span: 1.0,
                minimum_span_policy: worldrunner_render_shadow::MinimumSpanPolicy::ExtendNear,
                active_maximum: None,
                split_lambda: 0.5,
                split_policy: worldrunner_render_shadow::SplitPolicy::Practical,
            },
        })
        .unwrap();
        let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("failed SDSM mapping"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut pending = None;
        let prepared = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                pending = Some(PendingReduction {
                    buffer: buffer.clone(),
                    ticket: state.ticket(0),
                    transaction: Publications::for_encoder(encoder)?,
                    mapping: None,
                });
                Ok::<_, SdsmError>(())
            })
            .unwrap();
        let submission = prepared.submit().unwrap();
        buffer.destroy();
        let scope = context
            .device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        assert!(matches!(
            pending.unwrap().wait(context.device(), submission),
            Err(SdsmError::Map(_))
        ));
        let _ = pollster::block_on(scope.pop());
    }
}
