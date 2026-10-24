use crate::bridge_support::*;
use worldrunner_render_post::{AttachmentLoad, CopyOutput, contracts::*};

fn provider(device: &wgpu::Device, samples: u32, second: bool) -> CheckedDraw {
    let values = if second {
        "let z = 0.125 + f32(s) * 0.125; return Out(vec4<f32>(0., 1., 0., 1.), vec4<f32>(1., 0., 0., 1.), vec4<f32>(0., 0., 1., 1.), z);"
    } else {
        "let depths = array(0.75, 0.5, 0.25, 0.625); let color = select(vec4<f32>(0., 0., 1., 1.), vec4<f32>(1., 0., 0., 1.), s == 0u); return Out(color, vec4<f32>(1.), vec4<f32>(1., 0., 0., 1.), depths[s]);"
    };
    let shader = format!(
        "{VERTEX} struct Out {{ @location(0) a: vec4<f32>, @location(1) b: vec4<f32>, @location(2) c: vec4<f32>, @builtin(frag_depth) z: f32 }} @fragment fn fs(@builtin(sample_index) s: u32) -> Out {{ {values} }}"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    let targets = vec![
        Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8Unorm,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL
        });
        3
    ];
    let mut desc = descriptor(&module, samples);
    desc.fragment.as_mut().unwrap().targets = &targets;
    desc.depth_stencil = Some(wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });
    CheckedDraw::new(device, &desc, vec![], 0..3, 0..1).unwrap()
}
fn depth_snapshot(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    source: CheckedView,
    output: CheckedView,
    samples: u32,
    name: &'static str,
) -> TypedOperation {
    let group = CheckedBindings::build(
        device,
        registry,
        &[TextureBinding {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            view: source.clone(),
            samples,
            semantic: Semantic::DeviceDepth,
            kind: TextureBindingKind::Sampled(wgpu::TextureSampleType::Depth),
        }],
    )
    .unwrap();
    let (ty, body) = if samples == 1 {
        (
            "texture_depth_2d",
            "let z = textureLoad(t, vec2<i32>(0), 0);",
        )
    } else {
        (
            "texture_depth_multisampled_2d",
            "var z = 1.; for (var i = 0; i < 4; i++) { z = min(z, textureLoad(t, vec2<i32>(0), i)); }",
        )
    };
    let shader = format!(
        "{VERTEX} @group(0) @binding(0) var t: {ty}; @fragment fn fs() -> @location(0) vec4<f32> {{ {body} return vec4<f32>(z, 0., 0., 1.); }}"
    );
    let packet = draw(device, &shader, vec![group], 1);
    let mut raster = pass(name, output);
    raster
        .uses
        .push(ResourceUse::read(source, UseKind::Sampled));
    TypedOperation::new(
        raster.id,
        TypedWork::Draws {
            pass: raster,
            draws: vec![packet],
        },
    )
}
fn boundary(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    attachments: (&[wgpu::Texture], &wgpu::Texture),
    snapshots: &[wgpu::Texture],
    samples: u32,
    second: bool,
) -> Vec<TypedOperation> {
    let (targets, depth) = attachments;
    let version = if second { 2 } else { 1 };
    let colors: Vec<_> = targets
        .iter()
        .map(|t| view(registry, t, 0, version))
        .collect();
    let resolved: Vec<_> = snapshots[..3]
        .iter()
        .map(|t| view(registry, t, 0, 1))
        .collect();
    let depth_view = view(registry, depth, 0, version);
    let raster = boundary_pass(&colors, &resolved, depth_view.clone(), samples, second);
    let mut ops = vec![TypedOperation::new(
        raster.id,
        TypedWork::Draws {
            pass: raster,
            draws: vec![provider(device, samples, second)],
        },
    )];
    if samples == 1 {
        for (index, (color, output)) in colors.into_iter().zip(resolved).enumerate() {
            let name = if second {
                ["second color a", "second color b", "second color c"][index]
            } else {
                ["first color a", "first color b", "first color c"][index]
            };
            let mut raster = pass(name, output);
            raster
                .uses
                .push(ResourceUse::read(color.clone(), UseKind::Sampled));
            let packet = CopyOutput::checked_draw(device, registry, color, 1).unwrap();
            ops.push(TypedOperation::new(
                raster.id,
                TypedWork::Draws {
                    pass: raster,
                    draws: vec![packet],
                },
            ));
        }
    }
    let depth_output = view(registry, &snapshots[3], 0, 1);
    ops.push(depth_snapshot(
        device,
        registry,
        depth_view,
        depth_output,
        samples,
        if second {
            "second minimum depth"
        } else {
            "first minimum depth"
        },
    ));
    ops
}
#[test]
fn two_loaded_boundaries_preserve_color_and_minimum_depth_snapshots() {
    let (device, authority) = gpu();
    for samples in [1, 4] {
        let mut registry = ResourceRegistry::default();
        let targets: Vec<_> = (0..3)
            .map(|_| texture(&device, wgpu::TextureFormat::Rgba8Unorm, samples, 1))
            .collect();
        let depth = texture(&device, wgpu::TextureFormat::Depth32Float, samples, 1);
        let first: Vec<_> = (0..4)
            .map(|_| texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1))
            .collect();
        let second: Vec<_> = (0..4)
            .map(|_| texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1))
            .collect();
        let mut ops = boundary(
            &device,
            &mut registry,
            (&targets, &depth),
            &first,
            samples,
            false,
        );
        ops.extend(boundary(
            &device,
            &mut registry,
            (&targets, &depth),
            &second,
            samples,
            true,
        ));
        execute(&device, &authority, &registry, &ops);
        let (expected_first, expected_second) = expectations(samples);
        for ((first, second), (a, b)) in first
            .iter()
            .zip(&second)
            .zip(expected_first.into_iter().zip(expected_second))
        {
            assert_eq!(read(&device, &authority, first), a);
            assert_eq!(read(&device, &authority, second), b);
            assert_eq!(read(&device, &authority, first), a);
        }
    }
}

fn boundary_pass(
    colors: &[CheckedView],
    resolved: &[CheckedView],
    depth_view: CheckedView,
    samples: u32,
    second: bool,
) -> RasterPass {
    RasterPass {
        id: id(if second {
            "second providers"
        } else {
            "initial providers"
        }),
        colors: colors
            .iter()
            .zip(resolved)
            .map(|(color, output)| {
                Some(ColorAttachment {
                    view: color.clone(),
                    before: second.then_some(1),
                    load: if second {
                        AttachmentLoad::Load
                    } else {
                        AttachmentLoad::ClearColor(wgpu::Color::BLACK)
                    },
                    store: wgpu::StoreOp::Store,
                    resolve: (samples > 1).then_some(output.clone()),
                })
            })
            .collect(),
        depth: Some(DepthAttachment {
            view: depth_view,
            before: second.then_some(1),
            load: if second {
                AttachmentLoad::Load
            } else {
                AttachmentLoad::ClearDepth(1.)
            },
            store: wgpu::StoreOp::Store,
            read_only: false,
        }),
        uses: vec![],
        bindings: vec![],
    }
}

fn expectations(samples: u32) -> ([[u8; 4]; 4], [[u8; 4]; 4]) {
    let expected_first = if samples == 1 {
        [
            [255, 0, 0, 255],
            [255; 4],
            [255, 0, 0, 255],
            [191, 0, 0, 255],
        ]
    } else {
        [
            [64, 0, 191, 255],
            [255; 4],
            [255, 0, 0, 255],
            [64, 0, 0, 255],
        ]
    };
    let expected_second = if samples == 1 {
        [
            [0, 255, 0, 255],
            [255, 0, 0, 255],
            [0, 0, 255, 255],
            [32, 0, 0, 255],
        ]
    } else {
        [
            [0, 191, 64, 255],
            [255, 64, 64, 255],
            [64, 0, 191, 255],
            [32, 0, 0, 255],
        ]
    };
    (expected_first, expected_second)
}
