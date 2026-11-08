//! Receivers and specializations: the depth-aware AO gather receivers, and the pipeline
//! constants the frame's specular convention and winding select.

use wgpu::BindGroup;
use worldrunner_render_plan::{
    SceneDirectDiffuse, SceneRoughnessRemap, SceneSpecularConvention, SceneSpecularOcclusion,
    SceneSpecularVisibility, SceneSsaoReceiver, SceneSurfaceDisk,
};

use super::{Captured, Case, Rig, Source, Target, capture, drawable, scenes};
use crate::device::GpuContext;
use crate::dynamic::capture_tests::{CONFIGS, build, context, one_texel, supported};
use crate::frame::FrameUniform;

fn gather_ssao(context: &GpuContext) -> BindGroup {
    let device = context.device();
    let filtered = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture gather texel"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view: Vec<u8> = glam::Mat4::IDENTITY
        .to_cols_array()
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let uniform = crate::upload::upload_buffer(
        device,
        context.queue(),
        "capture gather view",
        wgpu::BufferUsages::UNIFORM,
        &view,
    );
    crate::ssao::receiver::group(
        device,
        &filtered,
        &uniform,
        SceneSsaoReceiver::DepthAwareGather,
    )
}

fn gather_reflection(context: &GpuContext) -> BindGroup {
    let device = context.device();
    let layout = crate::ssr::forward::receiver_layout(device, SceneSsaoReceiver::DepthAwareGather);
    let [cube, plane] = crate::water::receivers::empty_environment(device);
    let ambient = one_texel(device);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let intensity = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture intensity"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    let view = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture gather view"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("capture reflection gather"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&ambient),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&cube),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&plane),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: intensity.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: view.as_entire_binding(),
            },
        ],
    })
}

/// The gather receivers of SSAO and of the reflection forward pass, which select a sibling
/// source and a screen layout of their own.
fn gather(source: Source) -> usize {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    if !drawable(&context, source) {
        return 0;
    }
    let mut run = 0;
    for (name, index, group) in [
        ("ssao-gather-1x", 6, gather_ssao(&context)),
        ("reflection-gather-1x", 8, gather_reflection(&context)),
    ] {
        let config = &CONFIGS[index];
        if !supported(&context, config) {
            println!(
                "TERRAIN-CAPTURE config={name} source={} skipped",
                source.name()
            );
            continue;
        }
        let built = build(context.device(), config);
        let rig = Rig::new(
            &context,
            &built.pipelines,
            &source.scene(),
            (source, scenes::frame(scenes::pose())),
        )
        .receiving(SceneSsaoReceiver::DepthAwareGather);
        capture(
            &rig,
            &Case {
                name,
                source: source.name(),
                target: Target {
                    formats: &built.formats,
                    samples: config.samples,
                },
                pipelines: &built.pipelines,
                screen: Some(&group),
                shadow: None,
            },
        );
        run += 1;
    }
    println!(
        "TERRAIN-CAPTURE-SUMMARY set=gather source={} compared={run}",
        source.name()
    );
    run
}

#[test]
fn gather_receivers_match_the_fixed_pipeline_byte_for_byte_on_plain_terrain() {
    assert!(gather(Source::Plain) > 0);
}

#[test]
fn gather_receivers_match_the_fixed_pipeline_byte_for_byte_on_page_terrain() {
    assert!(gather(Source::Pages) > 0);
}

#[test]
fn gather_receivers_match_the_fixed_pipeline_byte_for_byte_on_layered_terrain() {
    gather(Source::Layered);
}

#[test]
fn gather_receivers_match_the_fixed_pipeline_byte_for_byte_on_layered_page_terrain() {
    gather(Source::LayeredPages);
}

/// Draws plain terrain on `hdr-1x` or `hdr-4x` with `frame`, whose convention and winding pick
/// the fragment constants and the pipeline's front face.
fn specialized(
    context: &GpuContext,
    (name, index): (&str, usize),
    frame: &FrameUniform,
) -> Captured {
    let config = &CONFIGS[index];
    let built = build(context.device(), config);
    let rig = Rig::new(
        context,
        &built.pipelines,
        &Source::Plain.scene(),
        (Source::Plain, *frame),
    );
    capture(
        &rig,
        &Case {
            name,
            source: Source::Plain.name(),
            target: Target {
                formats: &built.formats,
                samples: config.samples,
            },
            pipelines: &built.pipelines,
            screen: None,
            shadow: None,
        },
    )
}

fn convention(change: impl FnOnce(&mut SceneSpecularConvention)) -> SceneSpecularConvention {
    let mut convention = SceneSpecularConvention::default();
    change(&mut convention);
    convention
}

fn disk(radius_microradians: u32) -> SceneSurfaceDisk {
    SceneSurfaceDisk::new(1_000_000, 1_000_000)
        .unwrap()
        .with_angular_radius_microradians(radius_microradians)
        .unwrap()
}

/// The 12 roughness, visibility and occlusion combinations, the prepared Lambertian direct
/// diffuse, and the default and a wide disk radius.
fn conventions() -> Vec<(String, SceneSpecularConvention)> {
    let mut all = Vec::new();
    for (r, roughness) in [
        SceneRoughnessRemap::Perceptual045,
        SceneRoughnessRemap::Perceptual04,
        SceneRoughnessRemap::LinearAlpha,
    ]
    .into_iter()
    .enumerate()
    {
        for (v, visibility) in [
            SceneSpecularVisibility::SmithCorrelated,
            SceneSpecularVisibility::SchlickSmith,
        ]
        .into_iter()
        .enumerate()
        {
            for (o, occlusion) in [
                SceneSpecularOcclusion::AmbientOnly,
                SceneSpecularOcclusion::RoughnessAware,
            ]
            .into_iter()
            .enumerate()
            {
                all.push((
                    format!("hdr-1x-roughness{r}-visibility{v}-occlusion{o}"),
                    convention(|c| {
                        c.roughness = roughness;
                        c.visibility = visibility;
                        c.occlusion = occlusion;
                    }),
                ));
            }
        }
    }
    all.push((
        "hdr-1x-lambertian".to_owned(),
        convention(|c| c.direct_diffuse = SceneDirectDiffuse::LambertianPrepared),
    ));
    all.push((
        "hdr-1x-disk-default".to_owned(),
        convention(|c| c.surface_disk = Some(disk(2700))),
    ));
    all.push((
        "hdr-1x-disk-wide".to_owned(),
        convention(|c| c.surface_disk = Some(disk(17_453))),
    ));
    all
}

#[test]
fn terrain_matches_the_fixed_pipeline_for_every_specular_convention_and_winding() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut images = Vec::new();
    for (name, convention) in conventions() {
        let mut frame = scenes::frame(scenes::pose());
        frame.specular_convention = convention;
        images.push(specialized(&context, (&name, 2), &frame).images.remove(0));
    }
    // The constants must reach the shader: the combinations do not all draw alike.
    let first = &images[0];
    assert!(
        images.iter().any(|image| image != first),
        "no specular convention changed the pixels"
    );
    for (name, index) in [("hdr-1x-cw", 2), ("hdr-4x-cw", 3)] {
        if !supported(&context, &CONFIGS[index]) {
            continue;
        }
        let frame = scenes::mirrored(scenes::frame(scenes::pose()));
        assert_eq!(frame.front_face(), wgpu::FrontFace::Cw);
        specialized(&context, (name, index), &frame);
    }
}
