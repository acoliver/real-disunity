use std::ops::Index;
use std::sync::OnceLock;

use wgpu::{
    BindGroupLayout, ColorTargetState, ColorWrites, Device, Face, FragmentState, FrontFace,
    MultisampleState, PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, TextureFormat, VertexBufferLayout, VertexState,
    VertexStepMode,
};

use super::Depth;
use super::layouts::{frame_layout, material_layout};
use crate::geometry::{
    INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, previous_instance_attributes,
    vertex_attributes,
};
use crate::pipeline::{MaskCoverage, MaterialSampling, PipelineKey};
use crate::renderer::ScenePipelines;

const VERTEX_ENTRY_POINT: &str = "vs_main";

/// What the fixed path was built from: the frozen composed source, and the rest of the
/// description read from the pipelines selected for a frame.
pub(crate) struct Build<'a> {
    /// The object source, composed from the snapshots in `object/fixtures`. It decides the
    /// colour targets and whether the previous-instance stream is declared.
    pub source: &'a str,
    pub shadow: Option<&'a BindGroupLayout>,
    pub screen: Option<&'a BindGroupLayout>,
    pub options: ([u32; 7], FrontFace),
    pub target: (TextureFormat, u32),
    pub depth: Depth,
}

impl<'a> Build<'a> {
    /// The object source `pipelines` were built from, which is the live composition.
    pub(crate) fn live_source(pipelines: &'a ScenePipelines) -> &'a str {
        pipelines.specialization.parts().object_source
    }

    pub(crate) fn of(pipelines: &'a ScenePipelines, source: &'a str) -> Self {
        let parts = pipelines.specialization.parts();
        Self {
            source,
            shadow: parts.shadow,
            screen: parts.screen,
            options: parts.options,
            target: (pipelines.color_format, pipelines.samples),
            depth: pipelines.depth.into(),
        }
    }
}

/// The 32 object pipelines of one context, compiled the first time a draw needs them, as
/// `ObjectPipelines` did, and the layouts their bind groups are built against.
pub(crate) struct Pipelines {
    device: Device,
    layout: PipelineLayout,
    module: wgpu::ShaderModule,
    targets: Vec<Option<ColorTargetState>>,
    previous_transforms: bool,
    depth: Depth,
    samples: u32,
    front_face: FrontFace,
    constants: Vec<(&'static str, f64)>,
    variants: [OnceLock<RenderPipeline>; PipelineKey::ALL.len()],
}

impl Pipelines {
    /// Mirrors the object half of `ScenePipelines::with_constants`.
    pub(crate) fn build(device: &Device, build: &Build<'_>) -> Self {
        let mut constants = constants(build.options.0).to_vec();
        raster_constants(&mut constants, build.options.1);
        let (color_format, samples) = build.target;
        let frame_layout = frame_layout(device);
        let material_layout = material_layout(device);
        let groups = pipeline_groups(&frame_layout, &material_layout, build.shadow, build.screen);
        Self {
            device: device.clone(),
            layout: scene_layout(device, "environment", &groups),
            module: scene_shader(device, "environment", build.source),
            targets: source_targets(build.source, color_format),
            previous_transforms: build.source.contains(crate::velocity::MARKER),
            depth: build.depth,
            samples,
            front_face: build.options.1,
            constants,
            variants: std::array::from_fn(|_| OnceLock::new()),
        }
    }

    /// The colour targets every pipeline of this context writes.
    pub(crate) fn targets(&self) -> &[Option<ColorTargetState>] {
        &self.targets
    }
}

impl Index<usize> for Pipelines {
    type Output = RenderPipeline;

    fn index(&self, index: usize) -> &Self::Output {
        self.variants[index].get_or_init(|| {
            let key = PipelineKey::ALL[index];
            let vertices = vertex_attributes();
            let instances = instance_attributes();
            let previous = previous_instance_attributes();
            let mut buffers = vertex_buffers(&vertices, &instances).to_vec();
            if self.previous_transforms {
                buffers.push(Some(VertexBufferLayout {
                    array_stride: INSTANCE_STRIDE,
                    step_mode: VertexStepMode::Instance,
                    attributes: &previous,
                }));
            }
            let mut constants = self.constants.clone();
            constants.extend(fragment_constants(key, self.samples));
            build_pipeline(
                &self.device,
                &self.layout,
                &buffers,
                (key, self.samples, self.front_face, self.depth),
                FragmentState {
                    module: &self.module,
                    entry_point: Some(entry_point(key)),
                    compilation_options: PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets: &self.targets,
                },
            )
        })
    }
}

fn entry_point(key: PipelineKey) -> &'static str {
    match (key.relief, key.masked) {
        (true, true) => "fs_relief_masked",
        (true, false) => "fs_relief_opaque",
        (false, true) => "fs_masked",
        (false, false) => "fs_opaque",
    }
}

fn alpha_to_coverage(key: PipelineKey, samples: u32) -> bool {
    key.masked && key.coverage == MaskCoverage::Multisample && samples > 1
}

fn fragment_constants(key: PipelineKey, samples: u32) -> [(&'static str, f64); 2] {
    [
        (
            "SCENE_ALPHA_TO_COVERAGE",
            f64::from(alpha_to_coverage(key, samples)),
        ),
        (
            "SCENE_IMPOSTOR",
            f64::from(key.sampling == MaterialSampling::Impostor),
        ),
    ]
}

fn build_pipeline(
    device: &Device,
    layout: &PipelineLayout,
    buffers: &[Option<VertexBufferLayout<'_>>],
    (key, samples, front_face, depth): (PipelineKey, u32, FrontFace, Depth),
    fragment: FragmentState<'_>,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("environment"),
        layout: Some(layout),
        vertex: VertexState {
            module: fragment.module,
            entry_point: Some(VERTEX_ENTRY_POINT),
            compilation_options: PipelineCompilationOptions::default(),
            buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face,
            cull_mode: if key.double_sided {
                None
            } else {
                Some(Face::Back)
            },
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(depth.state()),
        multisample: MultisampleState {
            count: samples,
            alpha_to_coverage_enabled: alpha_to_coverage(key, samples),
            ..Default::default()
        },
        fragment: Some(fragment),
        multiview_mask: None,
        cache: None,
    })
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

fn color_target(format: TextureFormat) -> ColorTargetState {
    ColorTargetState {
        format,
        blend: None,
        write_mask: ColorWrites::ALL,
    }
}

/// The colour targets the object source writes: one, three for the reflection output, plus the
/// velocity target, and the formats the receiver metadata declares.
fn source_targets(source: &str, format: TextureFormat) -> Vec<Option<ColorTargetState>> {
    let reflection = source.contains("struct ReflectionOutput");
    let mut targets = if source.contains(crate::velocity::MARKER) {
        let mut targets = vec![Some(color_target(format)); if reflection { 3 } else { 1 }];
        targets.push(Some(color_target(worldrunner_render_post::VELOCITY_FORMAT)));
        targets
    } else if reflection {
        vec![Some(color_target(format)); 3]
    } else if source.contains(crate::scene_metadata::MERGED_MARKER) {
        vec![Some(color_target(format)); 2]
    } else {
        vec![Some(color_target(format))]
    };
    targets.extend(
        crate::scene_metadata::receiver::source_formats(source)
            .into_iter()
            .map(|format| Some(color_target(format))),
    );
    targets
}
