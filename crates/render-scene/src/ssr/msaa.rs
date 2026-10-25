use super::*;
use crate::water::snapshots::{SnapshotFeature, Snapshots};

fn depth_pipeline(
    context: &GpuContext,
    samples: u32,
    water: bool,
    mask: u32,
) -> wgpu::RenderPipeline {
    let shader = format!("
        @vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {{
            let p = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0))[index];
            return vec4<f32>(p, {}, 1.0);
        }}
        @fragment fn fs() -> @builtin(sample_mask) u32 {{ return {mask}u; }}
    ", if water { "0.9" } else { "0.98 + p.x * 0.01" });
    let module = context
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sloped depth regression"),
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });
    context
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sloped depth regression"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
}
fn draw_depth(
    encoder: &mut wgpu::CommandEncoder,
    depth: &wgpu::Texture,
    pipeline: &wgpu::RenderPipeline,
    clear: bool,
) {
    let view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("sloped depth regression"),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &view,
            depth_ops: Some(wgpu::Operations {
                load: if clear {
                    wgpu::LoadOp::Clear(1.0)
                } else {
                    wgpu::LoadOp::Load
                },
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.draw(0..3, 0..1);
}

fn assert_snapshot_sampling(snapshots: &Snapshots, samples: u32) {
    use crate::frame::metadata::MetadataSampling;
    assert_eq!(
        FrameProduct::prepass_depth().sampling(),
        MetadataSampling::PixelCenter
    );
    for (product, texture) in [
        (
            FrameProduct::opaque_depth(snapshots.samples),
            &snapshots.depth,
        ),
        (
            FrameProduct::final_depth(snapshots.samples),
            &snapshots.final_depth,
        ),
    ] {
        assert_eq!(
            product.sampling(),
            MetadataSampling::ConservativeResolve(samples),
            "forward snapshot coverage must retain its source sample count"
        );
        assert_eq!(product.sampling().samples(), 1);
        product.attachment(texture).unwrap();
    }
}

#[test]
fn gpu_sloped_receivers_keep_reflections_at_1x_and_4x_and_water_masks_partial_coverage() {
    let context = context();
    let size = [9, 9];
    for samples in [1, 4] {
        let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
        let (ssr, depth) = setup(&context, config);
        let snapshots = Snapshots::new(context.device(), config);
        assert_snapshot_sampling(&snapshots, samples);
        let source = AttachmentView::new(ResourceRef::new("scene depth"), &depth).unwrap();
        let group = snapshots.source_group(context.device(), &source, true);
        let slope = depth_pipeline(&context, samples, false, u32::MAX);
        let water = depth_pipeline(&context, samples, true, u32::MAX);
        let partial = depth_pipeline(&context, samples, true, 1);
        for water_pipeline in [None, Some(&water), Some(&partial)] {
            let mut encoder = context.submission().begin(context.device()).unwrap();
            let mut feature = SnapshotFeature {
                snapshots: snapshots.clone(),
                frame: frame(),
                color_source: None,
                depth_source: None,
            };
            let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
            feature.commit(&mut prepare);
            prepare.finish();
            draw_depth(&mut encoder, &depth, &slope, true);
            snapshot(
                &mut encoder,
                &snapshots.depth,
                &snapshots.depth_pipeline,
                &group,
            );
            if let Some(pipeline) = water_pipeline {
                draw_depth(&mut encoder, &depth, pipeline, false);
            }
            snapshot(
                &mut encoder,
                &snapshots.final_depth,
                &snapshots.depth_pipeline,
                &group,
            );
            encoder.finish().submit().unwrap();
            let result = super::regressions::resolve(
                &context,
                &ssr,
                &snapshots.depth,
                &snapshots.final_depth,
                false,
                samples,
            );
            assert_reflection(
                &context,
                &snapshots,
                &result,
                samples,
                water_pipeline.is_some(),
            );
        }
    }
}
fn snapshot(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::Texture,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) {
    worldrunner_render_post::fullscreen_pass(
        encoder,
        Some("matching snapshot reduction"),
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        pipeline,
        &[group],
    );
}

fn setup(context: &GpuContext, config: FrameConfig) -> (Ssr, wgpu::Texture) {
    let size = config.size;
    let samples = config.samples;
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(context.device(), config, settings(), &mut screen).unwrap();
    upload(context, &ssr.normals, &[0.0, 0.0, 1.0, 1.0].repeat(81));
    upload(context, &ssr.material, &[0.5, 0.5, 0.5, 0.0].repeat(81));
    // The centre prepass is independent of the nearest forward sample.
    upload(context, &ssr.hierarchy[0], &[5.0; 81]);
    for texture in &ssr.reflections {
        upload(
            context,
            texture,
            &[1.0; 4].repeat(usize::try_from(texture.width() * texture.height()).unwrap()),
        );
    }
    let depth = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("sloped forward depth"),
        size: wgpu::Extent3d {
            width: 9,
            height: 9,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let feature = ReflectionFeature {
        ssr: &ssr,
        frame: frame(),
        groups: Vec::new(),
        color: AttachmentView::new(ResourceRef::new("scene color"), &ssr.material).unwrap(),
    };
    let declaration = feature.schedule(&mut Vec::new(), &mut Vec::new()).unwrap();
    let resolve = declaration
        .passes
        .iter()
        .find(|pass| pass.slot == PassSlot::ScreenSpaceAfterTransparent)
        .unwrap();
    assert!(
        resolve
            .reads
            .contains(&ResourceRef::new("opaque snapshot depth"))
    );
    assert!(
        !resolve
            .reads
            .contains(&ResourceRef::new("linear camera depth"))
    );
    (ssr, depth)
}

fn assert_reflection(
    context: &GpuContext,
    snapshots: &Snapshots,
    result: &[u8],
    samples: u32,
    water: bool,
) {
    let centre = (4 * 9 + 4) * 8;
    let red = u16::from_le_bytes(result[centre..centre + 2].try_into().unwrap());
    if water {
        assert_eq!(
            red, 0,
            "even partially water-covered pixels conservatively exclude opaque SSR"
        );
    } else {
        assert_eq!(
            red,
            half(0.5),
            "no-water sloped receivers must retain the specular contribution"
        );
        assert_eq!(
            read(context, &snapshots.depth),
            read(context, &snapshots.final_depth)
        );
        if samples == 4 {
            let centre_depth = 1.0 / (1.0 / 0.1 - 0.98 * (1.0 / 0.1 - 1.0 / 200.0));
            let bytes = read(context, &snapshots.depth);
            let actual = f32::from_le_bytes(bytes[40 * 4..41 * 4].try_into().unwrap());
            assert!(
                (actual - centre_depth).abs() > 0.001,
                "fixture must expose different centre and MSAA sample depths"
            );
        }
    }
}
