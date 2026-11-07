use super::super::forward_rejection::{moving_frame, samples};
use super::*;

fn frozen(source: &str) -> String {
    // The complete production-assembly fingerprint guards pin every other source byte.
    source.replace("@invariant @builtin(position)", "@builtin(position)")
}

#[derive(Clone, Copy)]
pub(super) enum Transform {
    World,
    Mirrored,
    ClipSpace,
    OneUlpSeparated,
    CachedTerrain,
    Quads,
}

struct Pair {
    forward: ScenePipelines,
    metadata: ScenePipelines,
    screen: SceneMetadata,
    metadata_group: wgpu::BindGroup,
    forward_group: wgpu::BindGroup,
    reuse: bool,
}

impl Pair {
    fn new(context: &GpuContext, reuse: bool) -> Self {
        let device = context.device();
        let (object, terrain) = crate::ssr::forward::velocity_sources(
            (crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL),
            true,
            true,
        );
        let sources = if reuse {
            (object, terrain)
        } else {
            (frozen(&object), frozen(&terrain))
        };
        let layout = crate::ssr::forward::layout(device);
        let forward = ScenePipelines::with_screen_space(
            device,
            (TextureFormat::Rgba16Float, 1),
            &sources.0,
            &sources.1,
            None,
            Some(&layout),
        )
        .for_depth(if reuse {
            DepthMode::Reuse
        } else {
            DepthMode::Independent
        });
        let screen = SceneMetadata::new(device, SIZE, crate::ssr::neutral_ao()).unwrap();
        let (object, terrain) = crate::scene_metadata::sources(MetadataOutput::Merged);
        let sources = if reuse {
            (object, terrain)
        } else {
            (frozen(&object), frozen(&terrain))
        };
        let (metadata, metadata_group) =
            crate::scene_metadata::pipelines_from_sources(device, &screen.uniform, &sources);
        Self {
            forward,
            metadata: metadata.for_depth(if reuse {
                DepthMode::Prepass
            } else {
                DepthMode::Independent
            }),
            screen,
            metadata_group,
            forward_group: samples::screen_group(context, &layout),
            reuse,
        }
    }

    fn encode(
        &self,
        context: &GpuContext,
        gpu: &SceneGpu,
        frame: &FrameUniform,
        views: &[TextureView; 7],
    ) -> Vec<u8> {
        let prepass_depth = attachment(context.device(), TextureFormat::Depth32Float);
        let prepass_view = prepass_depth.create_view(&TextureViewDescriptor::default());
        let mut encoder = context.submission().begin(context.device()).unwrap();
        gpu.prepare_terrain_geometry(context.device(), &mut encoder, frame, SIZE)
            .unwrap();
        gpu.prepare_terrain_pages(context.device(), &mut encoder, frame, SIZE)
            .unwrap();
        let mut prepare =
            worldrunner_render_post::PrepareContext::new(context.device(), &mut encoder).unwrap();
        gpu.commit_frame(&mut prepare, *frame);
        self.screen.upload(&mut prepare, frame);
        prepare.finish();
        {
            let colors = colors(&views[4..6]);
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("paired metadata"),
                color_attachments: &colors,
                depth_stencil_attachment: Some(depth(
                    if self.reuse { &views[6] } else { &prepass_view },
                    false,
                    self.reuse,
                )),
                ..Default::default()
            });
            pass.set_stencil_reference(1);
            gpu.draw_prepass(&mut pass, &self.metadata, &self.metadata_group);
        }
        let (checkpoint, row, stride) = copy_bytes(
            context.device(),
            &mut encoder,
            if self.reuse {
                views[6].texture()
            } else {
                &prepass_depth
            },
        );
        {
            let colors = colors(&views[..4]);
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("depth ownership differential"),
                color_attachments: &colors,
                depth_stencil_attachment: Some(depth(&views[6], self.reuse, self.reuse)),
                ..Default::default()
            });
            pass.set_stencil_reference(1);
            pass.set_bind_group(3, &self.forward_group, &[]);
            gpu.draw_color(&mut pass, &self.forward, None);
        }
        encoder.finish().submit().unwrap();
        unpack(context.device(), &checkpoint, row, stride)
    }
}

pub(super) fn frame(
    context: &GpuContext,
    shot: (&RenderScene, CameraPose),
    options: (bool, f32),
) -> Vec<Vec<u8>> {
    transformed_frame(context, shot, options, Transform::World)
}

pub(super) fn transformed_frame(
    context: &GpuContext,
    shot: (&RenderScene, CameraPose),
    options: (bool, f32),
    transform: Transform,
) -> Vec<Vec<u8>> {
    let (reuse, jitter) = options;
    let pair = Pair::new(context, reuse);
    let scene = lit(shot.0, true);
    let mut gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &scene,
        &pair.forward,
    )
    .unwrap();
    let mut frame = moving_frame(shot.1, scene.effects());
    frame.view_projection.w_axis.x += jitter / 65.0;
    apply_transform(context, &mut gpu, &pair.forward, transform, &mut frame);
    let targets = [
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba16Float,
        TextureFormat::Rg16Float,
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba16Float,
        pair.forward.depth.format(),
    ]
    .map(|format| attachment(context.device(), format));
    let views = targets
        .each_ref()
        .map(|t| t.create_view(&TextureViewDescriptor::default()));
    let checkpoint = pair.encode(context, &gpu, &frame, &views);
    let mut products: Vec<_> = targets.iter().map(|target| read(context, target)).collect();
    products.push(checkpoint);
    products
}

fn apply_transform(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    transform: Transform,
    frame: &mut FrameUniform,
) {
    match transform {
        Transform::World => {}
        Transform::CachedTerrain => {
            let mut source = layered_terrain::page_source();
            source.sample = Box::new(|_| {
                vec![crate::terrain::pages::composition::LayerSample {
                    color: [100, 150, 70, 255],
                    normal: [0, 0, i16::MAX],
                    roughness: 160,
                    weight: 1,
                }]
            });
            gpu.enable_terrain_pages(context.device(), context.submission(), pipelines, source)
                .unwrap();
        }
        Transform::Quads => {
            gpu.set_dynamic_contribution(
                context.device(),
                context.submission(),
                pipelines,
                &[],
                &[crate::dynamic::DynamicQuad {
                    position: [0.0, -0.5, 0.7],
                    tint: [0.3, 0.8, 0.2, 0.5],
                    ..Default::default()
                }],
            )
            .unwrap();
        }
        Transform::Mirrored => {
            let model = crate::geometry::pack_model_instance(glam::Mat4::from_scale(
                glam::Vec3::new(-1.0, 1.0, 1.0),
            ));
            for batch in gpu.instances.batches.values() {
                context
                    .queue()
                    .write_buffer(batch.buffer.as_ref().unwrap(), 0, &model);
            }
        }
        Transform::ClipSpace | Transform::OneUlpSeparated => {
            if matches!(transform, Transform::OneUlpSeparated) {
                for (key, batch) in &gpu.instances.batches {
                    let z = if key.2 == 0 {
                        f32::from_bits(0.5_f32.to_bits() + 1)
                    } else {
                        0.5
                    };
                    let model = crate::geometry::pack_model_instance(glam::Mat4::from_translation(
                        glam::Vec3::new(0.0, 0.0, z),
                    ));
                    context
                        .queue()
                        .write_buffer(batch.buffer.as_ref().unwrap(), 0, &model);
                }
            }
            frame.view_projection = glam::Mat4::IDENTITY;
            frame.view = glam::Mat4::IDENTITY;
            let temporal = frame.temporal.as_mut().unwrap();
            temporal.unjittered_view_projection = frame.view_projection;
            temporal.previous_view_projection = frame.view_projection;
        }
    }
}

fn attachment(device: &Device, format: TextureFormat) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("depth ownership bytes"),
        size: Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn colors(views: &[TextureView]) -> Vec<Option<RenderPassColorAttachment<'_>>> {
    views
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
        .collect()
}

fn depth(view: &TextureView, load: bool, stencil: bool) -> RenderPassDepthStencilAttachment<'_> {
    RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(Operations {
            load: if load {
                LoadOp::Load
            } else {
                LoadOp::Clear(1.0)
            },
            store: StoreOp::Store,
        }),
        stencil_ops: stencil.then_some(Operations {
            load: if load { LoadOp::Load } else { LoadOp::Clear(0) },
            store: StoreOp::Store,
        }),
    }
}

pub(super) fn read(context: &GpuContext, texture: &Texture) -> Vec<u8> {
    if !texture.usage().contains(TextureUsages::COPY_SRC) {
        return super::super::forward_rejection::samples::read(context, texture);
    }
    let mut encoder = context
        .device()
        .create_command_encoder(&CommandEncoderDescriptor::default());
    let (buffer, row, stride) = copy_bytes(context.device(), &mut encoder, texture);
    context.queue().submit([encoder.finish()]);
    unpack(context.device(), &buffer, row, stride)
}

fn copy_bytes(
    device: &Device,
    encoder: &mut CommandEncoder,
    texture: &Texture,
) -> (Buffer, u32, u32) {
    let aspect = if texture.format().is_depth_stencil_format() {
        wgpu::TextureAspect::DepthOnly
    } else {
        wgpu::TextureAspect::All
    };
    let pixel_bytes = texture.format().block_copy_size(Some(aspect)).unwrap();
    let row = texture.width() * pixel_bytes;
    let stride = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("unpadded attachment bytes"),
        size: u64::from(stride * texture.height()),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect,
        },
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    (buffer, row, stride)
}

fn unpack(device: &Device, buffer: &Buffer, row: u32, stride: u32) -> Vec<u8> {
    map_pixels(device, buffer)
        .chunks_exact(stride as usize)
        .flat_map(|row_bytes| row_bytes[..row as usize].iter().copied())
        .collect()
}
