//! The fixed water body path as it is before water bodies draw through the program runtime.
//!
//! This is a frozen copy, kept as test-only code (it is declared from `water/tests.rs`) until
//! the fixed water pipelines are removed.
//! It is the reference of the byte-equality captures in `water::tests::capture_tests`: those
//! draw the same bodies through `WaterFeature::draw` and through this copy and compare every
//! attachment byte for byte.
//!
//! The source composition, pipeline builder and command sequence below are the ones `water.rs`
//! holds (`water_source`, `pipeline_with_source_depth`, the `TransparentWater` arm of
//! `WaterFeature::draw`), with these differences only:
//!
//! - The source is composed from the snapshots in `water/fixtures` and, for the shared files,
//!   from `object/fixtures`. The guards in `water::tests::program_tests` fail when the live
//!   WGSL or the live composition drifts from them.
//! - The pipelines are built here for each body's own key and are not read from `Water`'s
//!   pipeline fields, so this path keeps working when those fields go.
//!
//! The text specialisers (`shadow_response::specialize`, `vertex_seeds::specialize`), the
//! override constants (`terms::constants`, `terms::vertex_constants`), the vertex layouts
//! (`geometry::layouts`) and the bind groups, buffers and counts of the feature under test are
//! the live ones. Both paths share them; the captures compare what the two paths do with them.
//!
//! Do not edit the behaviour of this file. A change to water output is a change to what the
//! captures mean and is its own proposal.

use crate::renderer::depth::DepthMode;
use crate::water::{WaterFeature, endpoint, geometry, shadow_response, terms, vertex_seeds};

macro_rules! water_fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/", $name, ".wgsl"))
    };
}

macro_rules! shared_fixture {
    ($name:literal) => {
        include_str!(concat!("../object/fixtures/", $name, ".wgsl"))
    };
}

/// `water_source(shadows)` as it was at the freeze. The lighting crate's text ends in two blank
/// lines that its snapshot in `object/fixtures` leaves out, so they are added back here.
pub(super) fn source(shadows: bool) -> String {
    let shadow_source = if shadows {
        format!(
            "{}\n{}",
            shared_fixture!("shadow_crate"),
            shared_fixture!("shadows").replace("@group(2)", "@group(1)")
        )
    } else {
        shared_fixture!("unshadowed").to_owned()
    };
    format!(
        "{}\n{}\n{}\n{}\n{shadow_source}\n{}",
        concat!(shared_fixture!("lighting_crate"), "\n\n"),
        shared_fixture!("post_crate"),
        shared_fixture!("lighting").replace("@binding(0)", "@binding(3)"),
        shared_fixture!("camera_rays"),
        concat!(
            water_fixture!("ambient_visibility"),
            "\n",
            water_fixture!("optics"),
            "\n",
            water_fixture!("water"),
            "\n",
            water_fixture!("screen"),
            "\n",
            water_fixture!("terms"),
            "\n",
            water_fixture!("shadow_lookup"),
            "\n",
            water_fixture!("shadow_response"),
            "\n",
            water_fixture!("shadow_visibility")
        )
    )
}

/// What one pipeline is built from, as `pipeline_with_source_depth` takes it.
#[derive(Clone, Copy)]
struct Key {
    samples: u32,
    shadows: bool,
    options: [u32; 5],
    depth: DepthMode,
}

/// `pipeline_with_source_depth` as it was at the freeze, over the frozen source.
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    key: Key,
) -> wgpu::RenderPipeline {
    let Key {
        samples,
        shadows,
        options,
        depth,
    } = key;
    let buffers = geometry::layouts(options[4] & endpoint::SPECIALIZATION != 0);
    let composed = source(shadows);
    let source = shadow_response::specialize(&composed, options[4], shadows);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("planar water, frozen"),
        source: wgpu::ShaderSource::Wgsl(vertex_seeds::specialize(&source, options[4])),
    });
    let vertex_constants = terms::vertex_constants(options);
    let shadow_layout = crate::shadows::water_layout(device);
    let layouts = if shadows {
        vec![Some(layout), Some(&shadow_layout)]
    } else {
        vec![Some(layout)]
    };
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("water, frozen"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("water, frozen"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_water"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &vertex_constants,
                ..Default::default()
            },
            buffers: &buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_water"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &terms::constants(options),
                ..Default::default()
            },
            targets: &std::array::from_fn::<_, 4, _>(|_| {
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            }),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(depth.state(true)),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

/// The options `Water::feature_with_depth` selects pipelines by for `feature`'s frame: the four
/// specular options and the term mask of the bodies.
fn options(feature: &WaterFeature) -> [u32; 5] {
    let base = feature.frame.specular_convention.shader_options();
    [
        base[0],
        base[1],
        base[2],
        base[3],
        terms::mask(&feature.water.settings),
    ]
}

/// The pipeline of each body of `feature`, or `None` for a body with no vertices: the frame's
/// options, with the body's seed key added when it has one.
pub(super) fn pipelines(
    device: &wgpu::Device,
    feature: &WaterFeature,
) -> Vec<Option<wgpu::RenderPipeline>> {
    let water = &feature.water;
    let base = options(feature);
    water
        .bodies
        .iter()
        .zip(water.settings.iter())
        .map(|(body, settings)| {
            (body.vertices != 0).then(|| {
                let mut options = base;
                options[4] |= vertex_seeds::key(settings);
                let key = Key {
                    samples: water.samples,
                    shadows: feature.shadow.is_some(),
                    options,
                    depth: water.depth,
                };
                pipeline(device, &water.layout, key)
            })
        })
        .collect()
}

/// Records the bodies of `feature` as the `TransparentWater` arm of `WaterFeature::draw` does:
/// the shadow group at set 1 when there is one, then for each body with vertices its pipeline,
/// its group at set 0, its vertex buffer, its seed buffer at slot 1 when it has one, and a draw
/// of its vertices. `pipelines` are those of [`pipelines`] for the same feature. Returns the
/// number of bodies drawn.
pub(super) fn record(
    feature: &WaterFeature,
    pipelines: &[Option<wgpu::RenderPipeline>],
    pass: &mut wgpu::RenderPass<'_>,
) -> u32 {
    if let Some(shadow) = &feature.shadow {
        pass.set_bind_group(1, shadow, &[]);
    }
    let mut drawn = 0;
    let bodies = feature.water.bodies.iter().zip(&feature.groups);
    for ((body, group), pipeline) in bodies.zip(pipelines) {
        let Some(pipeline) = pipeline else {
            continue;
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.set_vertex_buffer(0, body.buffer.slice(..));
        if let Some(seed) = &body.seed {
            pass.set_vertex_buffer(1, seed.slice(..));
        }
        pass.draw(0..body.vertices, 0..1);
        drawn += 1;
    }
    drawn
}
