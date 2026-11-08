//! The fixed terrain path as it was before terrain drew through the program runtime.
//!
//! This is a frozen copy, kept as test-only code until the fixed pipelines are removed. It is
//! the reference of the byte-equality captures in `terrain::capture_tests`: those draw the same
//! terrain through the scene and through this copy and compare every attachment byte for byte.
//! The pipeline builders, layouts, bind group builders and the command sequence below are the
//! ones `renderer.rs`, `renderer/pages.rs`, `upload.rs`, `draw/resources.rs`, `draw.rs` and
//! `draw/streaming.rs` held, with these differences only:
//!
//! - The sources and options a pipeline set is built from are read from the [`ScenePipelines`]
//!   that were selected for the frame, instead of passed down from their constructor.
//! - Recording goes through [`Commands`], which a render pass and a counter implement, so a test
//!   can count the commands the fixed path issues.
//! - The page files are read from the frozen snapshots in `terrain/fixtures`.
//!
//! Do not edit the behaviour of this file. A change to terrain output is a change to what the
//! captures mean and is its own proposal.

use std::ops::Range;

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, ColorTargetState, ColorWrites,
    CompareFunction, DepthBiasState, DepthStencilState, Device, Face, FragmentState, FrontFace,
    IndexFormat, MultisampleState, PipelineCompilationOptions, PipelineLayout,
    PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline,
    RenderPipelineDescriptor, Sampler, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StencilState, Texture, TextureFormat, TextureSampleType, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexBufferLayout, VertexState, VertexStepMode,
};

use super::ScenePipelines;
use crate::draw::DrawCount;
use crate::geometry::{INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, vertex_attributes};
use crate::terrain::pages::layered_gpu::textures::{ARRAY_LIMIT, ResidentSource};

const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;
const TERRAIN_VERTEX_ENTRY_POINT: &str = "vs_terrain";
const TERRAIN_FRAGMENT_ENTRY_POINT: &str = "fs_terrain";
const PLANE_BINDING: u32 = 23;
const FOOTPRINT: &str = include_str!("../terrain/fixtures/footprint.wgsl");
const SAMPLING: &str = include_str!("../terrain/fixtures/sampling.wgsl");
const LAYERED: &str = include_str!("../terrain/fixtures/layered_gpu.wgsl");

/// The commands the fixed path issues into a pass.
pub(crate) trait Commands {
    fn set_pipeline(&mut self, pipeline: &RenderPipeline);
    fn set_bind_group(&mut self, index: u32, group: &BindGroup);
    fn set_vertex_buffer(&mut self, slot: u32, buffer: &Buffer, bytes: Option<Range<u64>>);
    fn set_index_buffer(&mut self, buffer: &Buffer, format: IndexFormat);
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>);
}

impl Commands for wgpu::RenderPass<'_> {
    fn set_pipeline(&mut self, pipeline: &RenderPipeline) {
        Self::set_pipeline(self, pipeline);
    }
    fn set_bind_group(&mut self, index: u32, group: &BindGroup) {
        Self::set_bind_group(self, index, group, &[]);
    }
    fn set_vertex_buffer(&mut self, slot: u32, buffer: &Buffer, bytes: Option<Range<u64>>) {
        Self::set_vertex_buffer(
            self,
            slot,
            bytes.map_or(buffer.slice(..), |r| buffer.slice(r)),
        );
    }
    fn set_index_buffer(&mut self, buffer: &Buffer, format: IndexFormat) {
        Self::set_index_buffer(self, buffer.slice(..), format);
    }
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>) {
        Self::draw_indexed(self, indices, base_vertex, instances);
    }
}

/// Counts the commands a recording issues, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counted {
    pub pipelines: u32,
    pub groups: u32,
    pub vertex_buffers: u32,
    pub index_buffers: u32,
    pub draws: u32,
}

impl Counted {
    pub(crate) const fn total(self) -> u32 {
        self.pipelines + self.groups + self.vertex_buffers + self.index_buffers + self.draws
    }
}

impl Commands for Counted {
    fn set_pipeline(&mut self, _: &RenderPipeline) {
        self.pipelines += 1;
    }
    fn set_bind_group(&mut self, _: u32, _: &BindGroup) {
        self.groups += 1;
    }
    fn set_vertex_buffer(&mut self, _: u32, _: &Buffer, _: Option<Range<u64>>) {
        self.vertex_buffers += 1;
    }
    fn set_index_buffer(&mut self, _: &Buffer, _: IndexFormat) {
        self.index_buffers += 1;
    }
    fn draw_indexed(&mut self, _: Range<u32>, _: i32, _: Range<u32>) {
        self.draws += 1;
    }
}

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
            (samples, &constants, build.options.1),
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

fn vertex_buffers<'a>(
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

fn scene_shader(device: &Device, label: &str, source: &str) -> wgpu::ShaderModule {
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
    target: (u32, &[(&str, f64)], FrontFace),
) -> RenderPipeline {
    let (samples, constants, front_face) = target;
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
        depth_stencil: Some(DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::Less),
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
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
    target: (u32, &[(&str, f64)], FrontFace),
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
    target: (u32, &[(&str, f64)], FrontFace),
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
    target: (u32, &[(&str, f64)], FrontFace),
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

fn terrain_layout_of(device: &Device, cached: bool, layered: bool) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("terrain"),
        entries: &terrain_entries(cached, layered),
    })
}

fn terrain_layout(device: &Device, cached: bool, layered: bool) -> BindGroupLayout {
    terrain_layout_of(device, cached, layered)
}

fn decal_texture_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: true },
            view_dimension: TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

fn terrain_entries(cached: bool, layered: bool) -> Vec<BindGroupLayoutEntry> {
    let mut entries = vec![BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }];
    entries.push(BindGroupLayoutEntry {
        binding: 1,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Uint,
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    });
    entries.push(terrain_plane_entry());
    if !layered {
        entries.extend((2..=4).map(decal_texture_entry));
    }
    entries.push(BindGroupLayoutEntry {
        binding: 5,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    for binding in 8..if layered { 11 } else { 8 } {
        entries.push(BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    if layered {
        entries.extend(draw_layout());
    }
    if cached {
        entries.retain(|entry| entry.binding != 1);
        entries.extend(cached_terrain_entries());
    }
    entries
}

fn terrain_plane_entry() -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding: PLANE_BINDING,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn cached_terrain_entries() -> Vec<BindGroupLayoutEntry> {
    vec![
        BindGroupLayoutEntry {
            binding: 6,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Uint,
                view_dimension: TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 7,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ]
}

fn draw_layout() -> Vec<BindGroupLayoutEntry> {
    let mut entries: Vec<_> = (0..ARRAY_LIMIT)
        .map(|index| BindGroupLayoutEntry {
            binding: 11 + index,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    entries.extend((21..23).map(|binding| BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }));
    entries
}

/// The resources the fixed bind groups are built from, held by the scene's terrain.
pub(crate) struct Resources<'a> {
    pub uniform: &'a Buffer,
    pub grid: &'a Texture,
    pub palettes: &'a [Texture],
    pub layered: Option<&'a ResidentSource>,
    pub planes: &'a Buffer,
    pub sampler: &'a Sampler,
}

/// Binds the terrain uniform, the type grid, the three palette arrays and the sampler into the
/// one group the terrain shader reads.
pub(crate) fn terrain_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    resources: &Resources<'_>,
    pages: Option<(&TextureView, &Buffer)>,
) -> BindGroup {
    let grid_view = resources
        .grid
        .create_view(&TextureViewDescriptor::default());
    let palette_views: Vec<TextureView> = resources
        .palettes
        .iter()
        .map(|texture| {
            texture.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::D2Array),
                ..TextureViewDescriptor::default()
            })
        })
        .collect();

    let mut entries = vec![
        BindGroupEntry {
            binding: 0,
            resource: resources.uniform.as_entire_binding(),
        },
        BindGroupEntry {
            binding: 1,
            resource: BindingResource::TextureView(&grid_view),
        },
    ];
    entries.push(BindGroupEntry {
        binding: PLANE_BINDING,
        resource: resources.planes.as_entire_binding(),
    });
    if pages.is_some() {
        entries.retain(|entry| entry.binding != 1);
    }
    for (slot, view) in palette_views.iter().enumerate() {
        entries.push(BindGroupEntry {
            binding: u32::try_from(slot + 2)
                .expect("three palette slots number far below u32::MAX"),
            resource: BindingResource::TextureView(view),
        });
    }
    entries.push(BindGroupEntry {
        binding: 5,
        resource: BindingResource::Sampler(resources.sampler),
    });
    if let Some((view, table)) = pages {
        entries.push(BindGroupEntry {
            binding: 6,
            resource: BindingResource::TextureView(view),
        });
        entries.push(BindGroupEntry {
            binding: 7,
            resource: table.as_entire_binding(),
        });
    }
    if let Some(source) = resources.layered {
        entries.retain(|entry| !matches!(entry.binding, 2..=4));
        entries.extend(layered_entries(source));
    }
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("terrain"),
        layout,
        entries: &entries,
    })
}

fn layered_entries(source: &ResidentSource) -> Vec<BindGroupEntry<'_>> {
    let mut entries: Vec<_> = source
        .buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| BindGroupEntry {
            binding: 8 + u32::try_from(index).expect("three inputs"),
            resource: buffer.as_entire_binding(),
        })
        .collect();
    entries.extend(
        source
            .views
            .iter()
            .enumerate()
            .map(|(index, view)| BindGroupEntry {
                binding: 11 + u32::try_from(index).expect("texture slot"),
                resource: BindingResource::TextureView(view),
            }),
    );
    entries.extend(
        source
            .samplers
            .iter()
            .enumerate()
            .map(|(index, sampler)| BindGroupEntry {
                binding: 21 + u32::try_from(index).expect("sampler slot"),
                resource: BindingResource::Sampler(sampler),
            }),
    );
    entries
}

/// One resident streamed chunk as the fixed draw reads it.
pub(crate) struct Chunk {
    /// The chunk's record in the instance buffer.
    pub record: u32,
    pub vertices: Buffer,
    pub indices: Buffer,
    pub index_count: u32,
}

/// The geometry the fixed draw records.
pub(crate) enum Geometry<'a> {
    Mesh {
        vertices: &'a Buffer,
        indices: &'a Buffer,
        index_count: u32,
    },
    Streaming(&'a [Chunk]),
}

/// Which pipeline and group the fixed draw selected.
pub(crate) enum Surface<'a> {
    Plain { layered: bool },
    Pages { layered: bool, group: &'a BindGroup },
}

/// `draw_terrain_color` and `draw_terrain_geometry` of the fixed path. Returns the draw count
/// and whether terrain was bound.
pub(crate) fn draw_terrain_color<C: Commands>(
    pass: &mut C,
    pipelines: &Pipelines,
    surface: &Surface<'_>,
    plain_group: &BindGroup,
    shadow: Option<&BindGroup>,
    (geometry, instances): (&Geometry<'_>, &Buffer),
) -> DrawCount {
    match surface {
        Surface::Pages { layered, group } => {
            pass.set_pipeline(if *layered {
                &pipelines
                    .layered
                    .as_ref()
                    .expect("validated layered device")
                    .cached
            } else {
                &pipelines.cached
            });
            pass.set_bind_group(1, group);
        }
        Surface::Plain { layered } => {
            pass.set_pipeline(if *layered {
                &pipelines
                    .layered
                    .as_ref()
                    .expect("validated layered device")
                    .terrain
            } else {
                &pipelines.terrain
            });
            pass.set_bind_group(1, plain_group);
        }
    }
    if let Some(shadow) = shadow {
        pass.set_bind_group(2, shadow);
    }
    draw_terrain_geometry(pass, geometry, instances)
}

fn draw_terrain_geometry<C: Commands>(
    pass: &mut C,
    geometry: &Geometry<'_>,
    instances: &Buffer,
) -> DrawCount {
    let mut count = DrawCount::default();
    match geometry {
        Geometry::Streaming(chunks) => {
            for chunk in *chunks {
                let offset = u64::from(chunk.record) * INSTANCE_STRIDE;
                pass.set_vertex_buffer(1, instances, Some(offset..offset + INSTANCE_STRIDE));
                pass.set_vertex_buffer(0, &chunk.vertices, None);
                pass.set_index_buffer(&chunk.indices, IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.index_count, 0, 0..1);
                count.draw(1);
            }
        }
        Geometry::Mesh {
            vertices,
            indices,
            index_count,
        } => {
            pass.set_vertex_buffer(0, vertices, None);
            pass.set_vertex_buffer(1, instances, None);
            pass.set_index_buffer(indices, IndexFormat::Uint32);
            pass.draw_indexed(0..*index_count, 0, 0..1);
            count = DrawCount {
                calls: 1,
                instances: 1,
            };
        }
    }
    count
}
