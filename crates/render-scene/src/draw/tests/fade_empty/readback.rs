use super::*;
use crate::draw::lod_material::FixedSink;
use crate::draw::object_runtime::PreparedSink;
use crate::pipeline::PipelineKey;

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Forward,
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
    formats: Vec<TextureFormat>,
    shadow: Option<[wgpu::RenderPipeline; PipelineKey::ALL.len()]>,
}

impl Setup {
    fn new(device: &Device, kind: Kind) -> Self {
        let hdr = TextureFormat::Rgba16Float;
        let mut setup = Self {
            pipelines: ScenePipelines::new(device, hdr),
            formats: vec![hdr],
            shadow: None,
        };
        if matches!(kind, Kind::Shadow) {
            setup.shadow = Some(crate::shadows::wind_test_pipelines(
                device,
                &setup.pipelines,
            ));
            setup.formats.clear();
        }
        setup
    }
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
    let forward = gpu.selected_pipelines(&setup.pipelines, &frame);
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
        if let Some(pipelines) = &setup.shadow {
            let mut sink = FixedSink::new(&mut pass, pipelines, &gpu.materials);
            gpu.record_selection(&mut sink, (shadow, &frame), &selected.ranges)
        } else {
            let mut sink = PreparedSink::new(&mut pass, &gpu.objects, forward.object_context());
            gpu.record_selection(&mut sink, (None, &frame), &selected.ranges)
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
