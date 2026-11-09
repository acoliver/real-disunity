use wgpu::{
    BindGroupLayout, ColorTargetState, ColorWrites, Device, Face, FragmentState, FrontFace,
    MultisampleState, PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, TextureFormat, VertexBufferLayout, VertexState,
    VertexStepMode,
};

use super::Depth;
use super::layouts::{terrain_layout, terrain_layout_of};
use crate::geometry::{INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, vertex_attributes};
use crate::renderer::ScenePipelines;

const TERRAIN_VERTEX_ENTRY_POINT: &str = "vs_terrain";
const TERRAIN_FRAGMENT_ENTRY_POINT: &str = "fs_terrain";
const FOOTPRINT: &str = include_str!("../../terrain/fixtures/footprint.wgsl");
const SAMPLING: &str = include_str!("../../terrain/fixtures/sampling.wgsl");
const LAYERED: &str = include_str!("../../terrain/fixtures/layered_gpu.wgsl");

/// What the fixed path was built from, read from the pipelines selected for a frame.
pub(crate) struct Build<'a> {
    pub frame_layout: &'a BindGroupLayout,
    pub material_layout: &'a BindGroupLayout,
    /// The object source, which decides the colour targets.
    pub object_source: &'a str,
    pub terrain_source: &'a str,
    pub shadow: Option<&'a BindGroupLayout>,
    pub screen: Option<&'a BindGroupLayout>,
    pub options: ([u32; 7], FrontFace),
    pub target: (TextureFormat, u32),
    pub depth: Depth,
}

impl<'a> Build<'a> {
    pub(crate) fn of(pipelines: &'a ScenePipelines) -> Self {
        let parts = pipelines.specialization.parts();
        Self {
            frame_layout: &pipelines.frame_layout,
            material_layout: &pipelines.material_layout,
            object_source: parts.object_source,
            terrain_source: parts.terrain_source,
            shadow: parts.shadow,
            screen: parts.screen,
            options: parts.options,
            target: (pipelines.color_format, pipelines.samples),
            depth: pipelines.depth.into(),
        }
    }
}

/// The four terrain pipelines of one context and the layouts their bind groups are built against.
pub(crate) struct Pipelines {
    pub terrain: RenderPipeline,
    pub layout: BindGroupLayout,
    pub cached_layout: BindGroupLayout,
    pub cached: RenderPipeline,
    pub layered: Option<LayeredPipelines>,
}

impl Pipelines {
    /// Mirrors the terrain half of `ScenePipelines::with_constants`.
    pub(crate) fn build(device: &Device, build: &Build<'_>) -> Self {
        let mut constants = constants(build.options.0).to_vec();
        raster_constants(&mut constants, build.options.1);
        let (color_format, samples) = build.target;
        let layout = terrain_layout(device, false, false);
        let mut groups = pipeline_groups(
            build.frame_layout,
            build.material_layout,
            build.shadow,
            build.screen,
        );
        let vertices = vertex_attributes();
        let instances = instance_attributes();
        let buffers = vertex_buffers(&vertices, &instances);
        let targets = source_targets(build.object_source, color_format);
        groups[1] = Some(&layout);
        let (terrain, cached_layout, cached, layered) = build_terrain_set(
            device,
            &groups,
            build.terrain_source,
            &buffers,
            &targets,
            (samples, &constants, build.options.1, build.depth),
        );
        Self {
            terrain,
            layout: layout.clone(),
            cached_layout,
            cached,
            layered,
        }
    }
}

fn raster_constants(constants: &mut Vec<(&'static str, f64)>, front_face: FrontFace) {
    if front_face == FrontFace::Cw {
        constants.push(("SCENE_RASTER_MIRRORED", 1.0));
    }
}

fn constants(options: [u32; 7]) -> [(&'static str, f64); 7] {
    [
        ("SCENE_ROUGHNESS_REMAP", f64::from(options[0])),
        ("SCENE_SPECULAR_VISIBILITY", f64::from(options[1])),
        ("SCENE_SPECULAR_OCCLUSION", f64::from(options[2])),
        ("SCENE_DIRECT_DIFFUSE", f64::from(options[3])),
        ("SCENE_DISK_INTENSITY", f64::from(options[4]) / 1_000_000.0),
        ("SCENE_DISK_PEAK", f64::from(options[5]) / 1_000_000.0),
        (
            "SCENE_DISK_RADIUS",
            if options[6] == 0 {
                0.0027
            } else {
                f64::from(options[6]) / 1_000_000.0
            },
        ),
    ]
}

pub(super) fn vertex_buffers<'a>(
    vertices: &'a [wgpu::VertexAttribute],
    instances: &'a [wgpu::VertexAttribute],
) -> [Option<VertexBufferLayout<'a>>; 2] {
    [
        Some(VertexBufferLayout {
            array_stride: VERTEX_STRIDE,
            step_mode: VertexStepMode::Vertex,
            attributes: vertices,
        }),
        Some(VertexBufferLayout {
            array_stride: INSTANCE_STRIDE,
            step_mode: VertexStepMode::Instance,
            attributes: instances,
        }),
    ]
}

pub(super) fn scene_shader(device: &Device, label: &str, source: &str) -> wgpu::ShaderModule {
    device.create_shader_module(ShaderModuleDescriptor {
        label: Some(label),
        source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(source)),
    })
}

fn scene_layout(
    device: &Device,
    label: &str,
    groups: &[Option<&BindGroupLayout>],
) -> PipelineLayout {
    device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: groups,
        immediate_size: 0,
    })
}

fn pipeline_groups<'a>(
    frame: &'a BindGroupLayout,
    material: &'a BindGroupLayout,
    shadow: Option<&'a BindGroupLayout>,
    screen: Option<&'a BindGroupLayout>,
) -> Vec<Option<&'a BindGroupLayout>> {
    let mut groups = vec![Some(frame), Some(material)];
    if let Some(shadow) = shadow {
        groups.push(Some(shadow));
    }
    if let Some(screen) = screen {
        groups.resize(3, None);
        groups.push(Some(screen));
    }
    groups
}

fn color_targets(format: TextureFormat) -> [Option<ColorTargetState>; 1] {
    [Some(ColorTargetState {
        format,
        blend: None,
        write_mask: ColorWrites::ALL,
    })]
}

fn source_targets(source: &str, format: TextureFormat) -> Vec<Option<ColorTargetState>> {
    if source.contains(crate::velocity::MARKER) {
        let reflection = source.contains("struct ReflectionOutput");
        let mut targets = if reflection {
            vec![color_targets(format)[0].clone(); 3]
        } else {
            color_targets(format).to_vec()
        };
        targets.push(Some(ColorTargetState {
            format: worldrunner_render_post::VELOCITY_FORMAT,
            blend: None,
            write_mask: ColorWrites::ALL,
        }));
        targets
    } else if source.contains("struct ReflectionOutput") {
        vec![color_targets(format)[0].clone(); 3]
    } else if source.contains(crate::scene_metadata::MERGED_MARKER) {
        vec![color_targets(format)[0].clone(); 2]
    } else {
        color_targets(format).to_vec()
    }
}

fn build_terrain(
    device: &Device,
    layout: &PipelineLayout,
    source: &str,
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
    target: (u32, &[(&str, f64)], FrontFace, Depth),
) -> RenderPipeline {
    let (samples, constants, front_face, depth) = target;
    let module = scene_shader(device, "terrain", source);

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("terrain"),
        layout: Some(layout),
        vertex: VertexState {
            module: &module,
            entry_point: Some(TERRAIN_VERTEX_ENTRY_POINT),
            compilation_options: PipelineCompilationOptions::default(),
            buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face,
            cull_mode: Some(Face::Back),
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(depth.state()),
        multisample: MultisampleState {
            count: samples,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: &module,
            entry_point: Some(TERRAIN_FRAGMENT_ENTRY_POINT),
            compilation_options: PipelineCompilationOptions {
                constants,
                ..Default::default()
            },
            targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn build_cached(
    device: &Device,
    groups: &[Option<&BindGroupLayout>],
    source: (&str, bool),
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
    target: (u32, &[(&str, f64)], FrontFace, Depth),
) -> (RenderPipeline, BindGroupLayout, RenderPipeline) {
    let (source, layered) = source;
    let source = if layered {
        layered_source(source)
    } else {
        source.to_owned()
    };
    let source = FOOTPRINT.to_owned() + &source;
    let source = source.as_str();
    let terrain_layout = scene_layout(device, "terrain", groups);
    let terrain = build_terrain(device, &terrain_layout, source, buffers, targets, target);
    let layout = terrain_layout_of(device, true, layered);
    let mut groups = groups.to_vec();
    groups[1] = Some(&layout);
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("cached terrain"),
        bind_group_layouts: &groups,
        immediate_size: 0,
    });
    let source = cached_source(source);
    let pipeline = build_terrain(device, &pipeline_layout, &source, buffers, targets, target);
    (terrain, layout, pipeline)
}

pub(crate) fn cached_source(source: &str) -> String {
    // Cached pages already contain surface selection; leave its source grid unbound.
    let source = source.replace(
        "let centre_surface = read_surface_at(i32(round(grid.x)), i32(round(grid.y)));",
        "let centre_surface = 0u;",
    );
    source.replace(
        "let blended = blend_surfaces(grid, world_xy, world_xy_dx, world_xy_dy, centre_surface);",
        "let blended = cached_surface(world_xy, world_xy_dx, world_xy_dy);",
    ) + SAMPLING
}

pub(crate) fn layered_source(source: &str) -> String {
    let mut shared = LAYERED.to_owned();
    for (input, output) in (0..3)
        .map(|i| (i, i + 8))
        .chain((5..17).map(|i| (i, i + 6)))
    {
        shared = shared.replace(
            &format!("@group(0) @binding({input})"),
            &format!("@group(1) @binding({output})"),
        );
    }
    // Layered draws use their source composer exclusively. Omitting the unused
    // conventional palettes leaves room for decals within the baseline stage limit.
    let start = source
        .find("fn blend_surfaces(")
        .expect("terrain blend function");
    let end = source[start..]
        .find("\nstruct TerrainVertexInput")
        .map(|offset| start + offset)
        .expect("terrain vertex entry");
    let source = format!(
        "{}fn blend_surfaces(grid: vec2<f32>, world_xy: vec2<f32>, world_xy_dx: vec2<f32>, world_xy_dy: vec2<f32>, centre_surface: u32) -> BlendedSurface {{ let sample = compose_layered(world_xy, world_xy_dx, world_xy_dy); return BlendedSurface(clamp(sample.albedo, vec3<f32>(0.0), vec3<f32>(1.0)), sample.normal, clamp(sample.roughness, 0.0, 1.0), clamp(sample.metallic, 0.0, 1.0)); }}\n{}",
        &source[..start],
        &source[end..]
    );
    // The frame's material mip bias finer-selects the screen-space gradients. The page bake
    // defines the same function as 1.0 because its footprints are not screen footprints.
    format!(
        "{source}\nfn layered_gradient_scale() -> f32 {{ return exp2(frame.lighting_options.w); }}\n{shared}"
    )
}

pub(crate) struct LayeredPipelines {
    pub layout: BindGroupLayout,
    pub cached_layout: BindGroupLayout,
    pub terrain: RenderPipeline,
    pub cached: RenderPipeline,
}

fn build_layered(
    device: &Device,
    groups: &[Option<&BindGroupLayout>],
    source: &str,
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
    target: (u32, &[(&str, f64)], FrontFace, Depth),
) -> Option<LayeredPipelines> {
    if !crate::terrain::pages::layered_gpu::supported(&device.limits()) {
        return None;
    }
    let layout = terrain_layout_of(device, false, true);
    let mut groups = groups.to_vec();
    groups[1] = Some(&layout);
    let (terrain, cached_layout, cached) =
        build_cached(device, &groups, (source, true), buffers, targets, target);
    Some(LayeredPipelines {
        layout,
        cached_layout,
        terrain,
        cached,
    })
}

fn build_terrain_set(
    device: &Device,
    groups: &[Option<&BindGroupLayout>],
    source: &str,
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
    target: (u32, &[(&str, f64)], FrontFace, Depth),
) -> (
    RenderPipeline,
    BindGroupLayout,
    RenderPipeline,
    Option<LayeredPipelines>,
) {
    let (terrain, layout, cached) =
        build_cached(device, groups, (source, false), buffers, targets, target);
    let layered = build_layered(device, groups, source, buffers, targets, target);
    (terrain, layout, cached, layered)
}
