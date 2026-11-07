use super::*;
use crate::pipeline::PipelineKey;
use crate::scene_metadata::{MetadataOutput, SceneMetadata};

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Forward,
    Velocity,
    Reflections,
    Metadata,
    Shadow,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Cull {
    Disabled,
    Enabled,
    ForceEmpty,
}

struct Setup {
    pipelines: ScenePipelines,
    group: Option<wgpu::BindGroup>,
    formats: Vec<TextureFormat>,
    shadow: Option<[wgpu::RenderPipeline; PipelineKey::ALL.len()]>,
}

impl Setup {
    fn new(device: &Device, kind: Kind) -> Self {
        let hdr = TextureFormat::Rgba16Float;
        let mut setup = Self {
            pipelines: ScenePipelines::new(device, hdr),
            group: None,
            formats: vec![hdr],
            shadow: None,
        };
        match kind {
            Kind::Forward => (),
            Kind::Velocity => {
                setup.pipelines = ScenePipelines::with_taa_velocity(device, hdr, 1).unwrap();
                setup.formats.push(worldrunner_render_post::VELOCITY_FORMAT);
            }
            Kind::Reflections => {
                let screen =
                    SceneMetadata::new(device, [TARGET; 2], crate::ssr::neutral_ao()).unwrap();
                let maps = crate::water::receivers::empty_environment(device);
                setup.group = Some(crate::ssr::forward::group(
                    device, &screen, &maps, 1_000_000,
                ));
                let (object, terrain) = crate::ssr::forward::velocity_sources(
                    (crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL),
                    true,
                    false,
                );
                setup.pipelines = ScenePipelines::with_screen_space(
                    device,
                    (hdr, 1),
                    &object,
                    &terrain,
                    None,
                    Some(&crate::ssr::forward::layout(device)),
                );
                setup.formats = vec![hdr, hdr, hdr, worldrunner_render_post::VELOCITY_FORMAT];
            }
            Kind::Metadata => {
                let uniform = metadata_uniform(device);
                let (pipelines, group) =
                    crate::scene_metadata::pipelines(device, &uniform, MetadataOutput::Merged);
                setup.pipelines = pipelines;
                setup.group = Some(group);
                setup.formats = vec![hdr; 2];
            }
            Kind::Shadow => {
                setup.shadow = Some(crate::shadows::wind_test_pipelines(
                    device,
                    &setup.pipelines,
                ));
                setup.formats.clear();
            }
        }
        setup
    }
}

fn metadata_uniform(device: &Device) -> Buffer {
    use wgpu::util::DeviceExt;
    let bytes: Vec<_> = [glam::Mat4::IDENTITY; 3]
        .into_iter()
        .flat_map(|matrix| matrix.to_cols_array())
        .chain([0.0; 8])
        .flat_map(f32::to_le_bytes)
        .collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fade fixture metadata camera"),
        contents: &bytes,
        usage: BufferUsages::UNIFORM,
    })
}

fn attachment(device: &Device, format: TextureFormat) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("fade fixture attachment"),
        size: Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn capture(
    context: &GpuContext,
    scene: &RenderScene,
    kind: Kind,
    cull: Cull,
) -> (Vec<Vec<u8>>, super::super::super::DrawCount) {
    let device = context.device();
    let setup = Setup::new(device, kind);
    let mut gpu = SceneGpu::upload(device, context.submission(), scene, &setup.pipelines).unwrap();
    gpu.instances.fade_cull_disabled = cull != Cull::Enabled;
    let frame = frame_from(CameraPose {
        eye_millimeters: [0, -4000, 1000],
        target_millimeters: [0, 0, 1000],
    });
    gpu.set_frame(context.queue(), frame);
    let colors: Vec<_> = setup
        .formats
        .iter()
        .map(|&format| attachment(device, format))
        .collect();
    let depth = attachment(device, DEPTH_FORMAT);
    let views: Vec<_> = colors
        .iter()
        .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
        .collect();
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let attachments: Vec<_> = views
        .iter()
        .map(|view| {
            Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })
        })
        .collect();
    let shadow = setup.shadow.as_ref().map(|_| (0, frame.view_projection));
    let mut selected = gpu.select_objects(shadow, &frame);
    if cull == Cull::ForceEmpty {
        selected.ranges.clear();
    }
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    let count = {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("fade fixture"),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &gpu.frame_group, &[]);
        if let Some(group) = &setup.group {
            pass.set_bind_group(3, group, &[]);
        }
        if let Some(pipelines) = &setup.shadow {
            gpu.record_selection(&mut pass, pipelines, (shadow, &frame), &selected.ranges)
        } else {
            gpu.record_selection(
                &mut pass,
                setup.pipelines.object_pipelines(),
                (None, &frame),
                &selected.ranges,
            )
        }
    };
    let buffers: Vec<_> = colors
        .iter()
        .chain([&depth])
        .map(|texture| copy(device, &mut encoder, texture))
        .collect();
    context.queue().submit([encoder.finish()]);
    (
        buffers
            .iter()
            .map(|buffer| map_pixels(device, buffer))
            .collect(),
        count,
    )
}

fn copy(device: &Device, encoder: &mut CommandEncoder, texture: &Texture) -> Buffer {
    let bytes = texture.format().block_copy_size(None).unwrap();
    let row = TARGET * bytes;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: None,
        size: u64::from(row * TARGET),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect: if texture.format() == DEPTH_FORMAT {
                wgpu::TextureAspect::DepthOnly
            } else {
                wgpu::TextureAspect::All
            },
            ..texture.as_image_copy()
        },
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    buffer
}
