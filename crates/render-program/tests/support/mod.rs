//! A device, a submission authority and the passes and readbacks the tests that draw over lent
//! resources share.

use wgpu::util::DeviceExt;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::{PreparedProgramDraw, ProgramRuntime};

pub struct Gpu {
    pub device: wgpu::Device,
    pub authority: SubmissionAuthority,
}

/// One colour attachment of a pass, cleared to `clear` first and resolved into `resolve` when
/// it is multisampled.
pub struct Attachment<'a> {
    pub view: &'a wgpu::TextureView,
    pub resolve: Option<&'a wgpu::TextureView>,
    pub clear: wgpu::Color,
}

impl Gpu {
    pub fn new() -> Self {
        let instance = headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("these tests need a GPU adapter");
        eprintln!("render-program tests adapter: {}", adapter.get_info().name);
        let (device, queue, _) =
            pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))
                .expect("the adapter gives a device");
        Self {
            device,
            authority: SubmissionAuthority::new(queue),
        }
    }

    pub fn runtime(&self) -> ProgramRuntime {
        ProgramRuntime::new(
            self.device.clone(),
            self.authority.clone(),
            AllocationBudget(1 << 20),
        )
    }

    pub fn buffer(&self, usage: wgpu::BufferUsages, bytes: &[u8]) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lent"),
                contents: bytes,
                usage,
            })
    }

    pub fn floats(&self, usage: wgpu::BufferUsages, values: &[f32]) -> wgpu::Buffer {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.buffer(usage, &bytes)
    }

    pub fn texture(&self, size: u32, format: wgpu::TextureFormat, samples: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Records one pass over `attachments` with `draws`, each retained on the frame, and
    /// submits it.
    pub fn pass(
        &self,
        attachments: &[Attachment<'_>],
        draws: &[&PreparedProgramDraw],
    ) -> wgpu::SubmissionIndex {
        self.pass_with(attachments, &[], draws)
    }

    /// Like [`pass`](Self::pass), with `groups` bound once at the start of the pass, before any
    /// draw records: the sets that the pass owner binds and the draws leave alone.
    pub fn pass_with(
        &self,
        attachments: &[Attachment<'_>],
        groups: &[(u32, &wgpu::BindGroup)],
        draws: &[&PreparedProgramDraw],
    ) -> wgpu::SubmissionIndex {
        let mut frame = self.authority.begin(&self.device).unwrap();
        for draw in draws {
            draw.retain(&frame).unwrap();
        }
        let colors: Vec<_> = attachments
            .iter()
            .map(|attachment| {
                Some(wgpu::RenderPassColorAttachment {
                    view: attachment.view,
                    depth_slice: None,
                    resolve_target: attachment.resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(attachment.clear),
                        store: wgpu::StoreOp::Store,
                    },
                })
            })
            .collect();
        {
            let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tests"),
                color_attachments: &colors,
                ..wgpu::RenderPassDescriptor::default()
            });
            for (set, group) in groups {
                pass.set_bind_group(*set, *group, &[]);
            }
            for draw in draws {
                draw.record(&mut pass);
            }
        }
        frame.finish().submit().unwrap()
    }

    /// The texels of a square single-sample `texture`, rows tightly packed.
    pub fn read(&self, texture: &wgpu::Texture) -> Vec<u8> {
        let size = texture.width();
        let texel = texture.format().block_copy_size(None).unwrap();
        let line = size * texel;
        let padded = line.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded * size),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut frame = self.authority.begin(&self.device).unwrap();
        frame.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(size),
                },
            },
            texture.size(),
        );
        let index = frame.finish().submit().unwrap();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, |result| result.unwrap());
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(index),
                timeout: None,
            })
            .unwrap();
        let mapped = staging.slice(..).get_mapped_range().unwrap().to_vec();
        staging.unmap();
        mapped
            .chunks_exact(padded as usize)
            .flat_map(|row| &row[..line as usize])
            .copied()
            .collect()
    }
}
