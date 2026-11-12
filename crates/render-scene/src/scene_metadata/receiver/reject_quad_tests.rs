//! The whole-quad vote in `card_reject.wgsl` over every 2x2 mask of hidden lanes.
use crate::device::GpuContext;
use crate::test_support::SceneTestSession;
use worldrunner_render_post::{FrameConfig, Readback};

/// One quad: a 2x2 target covered by one triangle. Each lane writes white unless the vote
/// discards it. Relief depth is 0.5; stored depth 0 puts a lane behind, 1 keeps it in front.
const HARNESS: &str = "
var<private> scene_receiver_pixel: vec2<i32>;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.5, 1.0);
}

@fragment
fn fs(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    scene_receiver_pixel = vec2<i32>(position.xy);
    if scene_receiver_hidden(0.5) {
        discard;
    }
    return vec4<f32>(1.0);
}
";

const TARGET: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("card reject quad vote"),
        source: wgpu::ShaderSource::Wgsl(
            format!("{HARNESS}\n{}", include_str!("card_reject.wgsl")).into(),
        ),
    });
    let depth = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("card reject quad depth"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 5,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("card reject quad vote"),
        bind_group_layouts: &[None, None, Some(&depth)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("card reject quad vote"),
        layout: Some(&layout),
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
            targets: &[Some(TARGET.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, depth)
}

/// Stored depth per lane, row-major: bit `y * 2 + x` of `hidden` puts that lane behind.
fn stored_depth(context: &GpuContext, hidden: u8) -> wgpu::Texture {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("card reject quad stored depth"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth16Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texels: Vec<u8> = (0..4)
        .flat_map(|lane| {
            if hidden >> lane & 1 == 1 {
                0_u16
            } else {
                u16::MAX
            }
            .to_le_bytes()
        })
        .collect();
    context.queue().write_texture(
        texture.as_image_copy(),
        &texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(2),
        },
        texture.size(),
    );
    texture
}

/// Draws the quad over `hidden` and returns which lanes wrote, row-major.
fn written_lanes(
    context: &GpuContext,
    vote: &(wgpu::RenderPipeline, wgpu::BindGroupLayout),
    hidden: u8,
) -> [bool; 4] {
    let device = context.device();
    let stored = stored_depth(context, hidden);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("card reject quad depth"),
        layout: &vote.1,
        entries: &[wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::TextureView(
                &stored.create_view(&wgpu::TextureViewDescriptor::default()),
            ),
        }],
    });
    let target = worldrunner_render_post::color_texture(device, "card reject quad", [2, 2], TARGET);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    context
        .submission()
        .prepare(device, |encoder, _| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("card reject quad vote"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&vote.0);
            pass.set_bind_group(2, &bind_group, &[]);
            pass.draw(0..3, 0..1);
            Ok::<_, ()>(())
        })
        .unwrap()
        .submit()
        .unwrap();
    let mut readback = Readback::new(device, FrameConfig::new([2, 2], TARGET, 1), 4096).unwrap();
    let pixels = readback
        .read(device, context.submission(), &target)
        .unwrap();
    std::array::from_fn(|lane| pixels[lane * 4 + 3] == u8::MAX)
}

#[test]
fn only_a_quad_with_every_lane_hidden_discards() {
    let _session = SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let vote = pipeline(context.device());
    for hidden in 0_u8..16 {
        let expected = if hidden == 0b1111 {
            [false; 4]
        } else {
            [true; 4]
        };
        assert_eq!(
            written_lanes(&context, &vote, hidden),
            expected,
            "hidden lanes {hidden:#06b} (bit y * 2 + x)"
        );
    }
}
