use crate::bridge_support::*;
use worldrunner_render_post::{AttachmentLoad, contracts::*};

#[test]
fn checked_material_and_comparison_samplers_render() {
    let (device, authority) = gpu();
    for kind in [
        wgpu::SamplerBindingType::NonFiltering,
        wgpu::SamplerBindingType::Filtering,
        wgpu::SamplerBindingType::Comparison,
    ] {
        let comparison = kind == wgpu::SamplerBindingType::Comparison;
        let mut registry = ResourceRegistry::default();
        let format = if comparison {
            wgpu::TextureFormat::Depth32Float
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let input = texture(&device, format, 1, 1);
        let source = view(&mut registry, &input, 0, 1);
        let init = initialize(source.clone(), comparison);
        let group = sampled_group(&device, &registry, source.clone(), kind);
        let body = if comparison {
            "@group(0) @binding(0) var t: texture_depth_2d; @group(0) @binding(1) var s: sampler_comparison; @fragment fn fs() -> @location(0) vec4<f32> { let v = textureSampleCompare(t, s, vec2<f32>(0.5), 0.1); return vec4<f32>(v, 0., 0., 1.); }"
        } else {
            "@group(0) @binding(0) var t: texture_2d<f32>; @group(0) @binding(1) var s: sampler; @fragment fn fs() -> @location(0) vec4<f32> { return textureSample(t, s, vec2<f32>(0.5)); }"
        };
        let packet = draw(&device, &format!("{VERTEX} {body}"), vec![group], 1);
        let output = texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
        let target = view(&mut registry, &output, 0, 1);
        let mut raster = pass("sampled material", target);
        raster
            .uses
            .push(ResourceUse::read(source, UseKind::Sampled));
        execute(
            &device,
            &authority,
            &registry,
            &[
                TypedOperation::new(
                    init.id,
                    TypedWork::Draws {
                        pass: init,
                        draws: vec![],
                    },
                ),
                TypedOperation::new(
                    raster.id,
                    TypedWork::Draws {
                        pass: raster,
                        draws: vec![packet],
                    },
                ),
            ],
        );
        assert_eq!(
            read(&device, &authority, &output),
            if comparison {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            }
        );
    }
}

fn initialize(source: CheckedView, comparison: bool) -> RasterPass {
    if comparison {
        RasterPass {
            id: id("comparison input"),
            colors: vec![],
            depth: Some(DepthAttachment {
                view: source,
                before: None,
                load: AttachmentLoad::ClearDepth(0.25),
                store: wgpu::StoreOp::Store,
                read_only: false,
            }),
            uses: vec![],
            bindings: vec![],
        }
    } else {
        let mut init = pass("sampled input", source);
        init.colors[0].as_mut().unwrap().load = AttachmentLoad::ClearColor(wgpu::Color::GREEN);
        init
    }
}

fn sampled_group(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    source: CheckedView,
    kind: wgpu::SamplerBindingType,
) -> CheckedBindings {
    let comparison = kind == wgpu::SamplerBindingType::Comparison;
    let filter = if kind == wgpu::SamplerBindingType::NonFiltering {
        wgpu::FilterMode::Nearest
    } else {
        wgpu::FilterMode::Linear
    };
    let sampler = CheckedSampler::new(
        device,
        &wgpu::SamplerDescriptor {
            mag_filter: filter,
            min_filter: filter,
            compare: comparison.then_some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        },
    );
    assert_eq!(sampler.kind(), kind);
    CheckedBindings::build_with_resources(
        device,
        registry,
        &[TextureBinding {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            view: source,
            samples: 1,
            semantic: if comparison {
                Semantic::DeviceDepth
            } else {
                Semantic::LinearColor
            },
            kind: TextureBindingKind::Sampled(if comparison {
                wgpu::TextureSampleType::Depth
            } else {
                wgpu::TextureSampleType::Float { filterable: true }
            }),
        }],
        &[],
        &[SamplerBinding {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            sampler,
        }],
    )
    .unwrap()
}
