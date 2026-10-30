//! Building the render pipelines and depth attachment the scene draws through.
//!
//! Everything upstream of here has described GPU state without ever handing it
//! to a driver for judgement: [`crate::upload`] declares bind group layouts,
//! [`crate::geometry`] declares vertex attribute locations and strides, and
//! [`crate::shader`] declares the bindings and locations the WGSL expects. Each
//! of those is a claim about what the other two are doing, checked so far only
//! by us agreeing with ourselves.
//!
//! Pipeline creation is where those claims meet. An explicit
//! [`wgpu::PipelineLayout`] is supplied rather than letting wgpu deduce one
//! from the shader, precisely because a deduced layout cannot disagree: it is
//! derived from the module and would silently accommodate whatever the shader
//! happened to declare, leaving a mismatch to surface later as a failed bind or
//! a wrongly-fed attribute. Stating the layout means a divergence between our
//! Rust and our WGSL is a creation-time error naming the binding.
//!
//! # Why the four pipelines share one layout
//!
//! The variants differ only in cull mode and fragment entry point, never in
//! resources, so they can share a single pipeline layout. That is worth doing
//! for more than tidiness: pipelines built from the same layout do not
//! invalidate each other's bind groups, so the frame uniform is bound once per
//! pass and survives every pipeline switch within it.
//!
//! # Why terrain's different layout does not break that
//!
//! Terrain cannot share the layout: it binds a grid, three texture arrays and
//! its own uniform at group 1, where the environment binds a material. But the
//! property that matters is narrower than "one layout". Bind groups survive a
//! pipeline change for every slot up to the first the two layouts disagree on,
//! and both layouts hold the *same* [`BindGroupLayout`] object at group 0, so
//! they agree there and the frame binding survives the switch to terrain and
//! back. Only group 1 is invalidated, and both paths set group 1 themselves
//! anyway. This is why `frame_layout` is created once and shared rather than
//! built again for the terrain layout: two structurally identical layouts would
//! also satisfy the rule, but sharing the object makes the agreement impossible
//! to break by editing one of them.
//!
//! # Depth
//!
//! Depth is a plain 32-bit float with no stencil, because nothing here uses
//! stencil and the alternative worth considering, 24-bit unorm depth, spends
//! its precision badly for this world: the authored terrain spans kilometres
//! while the near plane is governed in millimetres, and that ratio is exactly
//! what drives depth fighting on distant coplanar surfaces.

mod objects;
mod pages;
mod specular;
mod unlit;
pub(crate) use objects::ObjectPipelines;
pub(crate) use specular::constants as specular_constants;

use std::borrow::Cow;

use wgpu::{
    BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Device, Extent3d, Face, FragmentState, FrontFace, MultisampleState,
    PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, StencilState, Texture, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, VertexBufferLayout, VertexState,
    VertexStepMode,
};

use crate::geometry::{INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, vertex_attributes};
use crate::pipeline::PipelineKey;
use crate::shader::{self, ENVIRONMENT_WGSL, TERRAIN_WGSL};
use crate::upload::{
    frame_bind_group_layout, material_bind_group_layout, terrain_bind_group_layout,
};
use crate::view::Viewport;

/// Depth format every pipeline here writes.
pub const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

/// The complete, closed set of pipelines a governed scene can require.
#[derive(Clone, Debug)]
pub struct ScenePipelines {
    pub(crate) color_format: TextureFormat,
    pub(crate) samples: u32,
    pub(crate) ssao: bool,
    velocity: bool,
    frame_layout: BindGroupLayout,
    material_layout: BindGroupLayout,
    terrain_layout: BindGroupLayout,
    pipelines: ObjectPipelines,
    terrain: RenderPipeline,
    pub(crate) cached_terrain: RenderPipeline,
    pub(crate) layered: Option<pages::LayeredPipelines>,
    pub(crate) cached_terrain_layout: BindGroupLayout,
    unlit_alpha: RenderPipeline,
    specialization: specular::Specialization,
}

impl ScenePipelines {
    /// Builds shared layouts and terrain pipelines for one colour target format.
    ///
    /// Material variants are shared and compiled on demand. Relief variants
    /// must not add shader compilation cost to scenes that never use them.
    #[must_use]
    pub fn new(device: &Device, color_format: TextureFormat) -> Self {
        Self::with_environment(
            device,
            (color_format, 1),
            ENVIRONMENT_WGSL,
            TERRAIN_WGSL,
            None,
        )
    }

    pub fn with_samples(
        device: &Device,
        color_format: TextureFormat,
        samples: u32,
    ) -> Result<Self, worldrunner_render_post::PostError> {
        worldrunner_render_post::FrameConfig::new([1, 1], color_format, samples)
            .validate_device(device)?;
        Ok(Self::with_environment(
            device,
            (color_format, samples),
            ENVIRONMENT_WGSL,
            TERRAIN_WGSL,
            None,
        ))
    }

    /// Pipelines whose object and terrain draws also write the object-motion residual to a
    /// second `Rg16Float` colour target, for a frame renderer with TAA resident.
    ///
    /// The pass drawing with them must bind the frame renderer's velocity attachment, which
    /// [`crate::compositor::encode_scene`] does; other passes cannot use these pipelines.
    pub fn with_taa_velocity(
        device: &Device,
        color_format: TextureFormat,
        samples: u32,
    ) -> Result<Self, worldrunner_render_post::PostError> {
        worldrunner_render_post::FrameConfig::new([1, 1], color_format, samples)
            .validate_device(device)?;
        Ok(Self::with_environment(
            device,
            (color_format, samples),
            &crate::velocity::object_source(ENVIRONMENT_WGSL),
            &crate::velocity::terrain_source(TERRAIN_WGSL),
            None,
        ))
    }

    /// Whether these pipelines write the velocity target.
    #[must_use]
    pub const fn writes_velocity(&self) -> bool {
        self.velocity
    }

    pub fn with_ssao(
        device: &Device,
        format: TextureFormat,
        samples: u32,
    ) -> Result<Self, worldrunner_render_post::PostError> {
        Self::ambient(device, (format, samples), false)
    }

    /// [`Self::with_ssao`] pipelines whose object and terrain draws also write the object-motion
    /// residual, for a frame renderer with TAA resident and SSAO without SSR.
    ///
    /// The SSAO metadata prepass keeps its own pipelines and writes no velocity. The pass drawing
    /// with these pipelines must bind the frame renderer's velocity attachment, which
    /// [`crate::compositor::encode_scene`] does.
    pub fn with_ssao_taa_velocity(
        device: &Device,
        format: TextureFormat,
        samples: u32,
    ) -> Result<Self, worldrunner_render_post::PostError> {
        Self::ambient(device, (format, samples), true)
    }

    fn ambient(
        device: &Device,
        (format, samples): (TextureFormat, u32),
        velocity: bool,
    ) -> Result<Self, worldrunner_render_post::PostError> {
        if format != TextureFormat::Rgba16Float {
            return Err(worldrunner_render_post::PostError::UnsupportedProfile(
                "SSAO requires HDR",
            ));
        }
        worldrunner_render_post::FrameConfig::new([1, 1], format, samples)
            .validate_device(device)?;
        let layout = crate::ssao::ambient_layout(device);
        let (object, terrain) = if velocity {
            (
                crate::velocity::object_source(ENVIRONMENT_WGSL),
                crate::velocity::terrain_source(TERRAIN_WGSL),
            )
        } else {
            (ENVIRONMENT_WGSL.to_owned(), TERRAIN_WGSL.to_owned())
        };
        Ok(Self::with_screen_space(
            device,
            (format, samples),
            &crate::ssao::ambient_source(&object),
            &crate::ssao::ambient_source(&terrain),
            None,
            Some(&layout),
        ))
    }

    pub(crate) fn with_environment(
        device: &Device,
        target: (TextureFormat, u32),
        source: &str,
        terrain_source: &str,
        shadow_layout: Option<&BindGroupLayout>,
    ) -> Self {
        Self::with_screen_space(device, target, source, terrain_source, shadow_layout, None)
    }

    pub(crate) fn with_screen_space(
        device: &Device,
        target: (TextureFormat, u32),
        source: &str,
        terrain_source: &str,
        shadow_layout: Option<&BindGroupLayout>,
        screen_layout: Option<&BindGroupLayout>,
    ) -> Self {
        Self::with_constants(
            device,
            target,
            (source, terrain_source),
            shadow_layout,
            screen_layout,
            ([0; 7], FrontFace::Ccw),
        )
    }

    fn with_constants(
        device: &Device,
        target: (TextureFormat, u32),
        sources: (&str, &str),
        shadow_layout: Option<&BindGroupLayout>,
        screen_layout: Option<&BindGroupLayout>,
        options: ([u32; 7], FrontFace),
    ) -> Self {
        let (source, terrain_source) = sources;
        let mut constants = specular::constants(options.0).to_vec();
        specular::raster_constants(&mut constants, options.1);
        let (color_format, samples) = target;
        let module = scene_shader(device, "environment", source);
        let frame_layout = frame_bind_group_layout(device);
        let material_layout = material_bind_group_layout(device);
        let terrain_layout = terrain_bind_group_layout(device);
        let mut groups = pipeline_groups(
            &frame_layout,
            &material_layout,
            shadow_layout,
            screen_layout,
        );
        let layout = scene_layout(device, "environment", &groups);

        let vertex_attributes = vertex_attributes();
        let instance_attributes = instance_attributes();
        let buffers = vertex_buffers(&vertex_attributes, &instance_attributes);
        let targets = source_targets(source, color_format);

        let pipelines = ObjectPipelines::new(
            device,
            &layout,
            &targets,
            &module,
            (samples, &constants, options.1),
            source.contains(crate::velocity::MARKER),
        );
        groups[1] = Some(&terrain_layout);
        let (terrain, cached_terrain_layout, cached_terrain, layered) = pages::build_terrain_set(
            device,
            &groups,
            terrain_source,
            &buffers,
            &targets,
            (samples, &constants, options.1),
        );
        Self {
            layered,
            specialization: specular::Specialization::new(
                device,
                source,
                terrain_source,
                shadow_layout,
                screen_layout,
                options,
            ),
            cached_terrain_layout,
            cached_terrain,
            color_format,
            samples,
            ssao: screen_layout.is_some(),
            velocity: source.contains(crate::velocity::MARKER),
            frame_layout,
            material_layout,
            terrain_layout,
            pipelines,
            terrain,
            unlit_alpha: unlit::build(device, target, &layout, &buffers, &module, &targets),
        }
    }

    pub const fn unlit_alpha(&self) -> &RenderPipeline {
        &self.unlit_alpha
    }

    pub(crate) const fn object_pipelines(&self) -> &ObjectPipelines {
        &self.pipelines
    }

    /// Returns the pipeline one material's state requires.
    #[must_use]
    pub fn get(&self, key: PipelineKey) -> &RenderPipeline {
        &self.pipelines[key.index()]
    }

    /// Returns the layout frame bind groups must be built against.
    #[must_use]
    pub const fn frame_layout(&self) -> &BindGroupLayout {
        &self.frame_layout
    }

    /// Returns the layout material bind groups must be built against.
    #[must_use]
    pub const fn material_layout(&self) -> &BindGroupLayout {
        &self.material_layout
    }

    /// Returns the pipeline the terrain surface draws through.
    #[must_use]
    pub const fn terrain(&self) -> &RenderPipeline {
        &self.terrain
    }

    /// Returns the layout terrain bind groups must be built against.
    #[must_use]
    pub const fn terrain_layout(&self) -> &BindGroupLayout {
        &self.terrain_layout
    }
}

pub(crate) fn vertex_buffers<'a>(
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

/// The third vertex buffer of velocity pipelines: each instance's previous-frame transform, laid
/// out like the instance buffer so one stride and one set of offsets serve both.
pub(crate) fn previous_instance_buffer(
    attributes: &[wgpu::VertexAttribute],
) -> VertexBufferLayout<'_> {
    VertexBufferLayout {
        array_stride: INSTANCE_STRIDE,
        step_mode: VertexStepMode::Instance,
        attributes,
    }
}

/// Builds the one pipeline a single material state requires.
///
/// Only `cull_mode` and the fragment entry point vary with the key. Everything
/// else is shared deliberately: a variant that quietly differed in depth
/// comparison or colour write mask would shade correctly and composite wrongly,
/// which is far harder to see than a shader that fails to compile.
pub(crate) fn build(
    device: &Device,
    key: PipelineKey,
    layout: &PipelineLayout,
    buffers: &[Option<VertexBufferLayout<'_>>],
    target: (u32, FrontFace),
    fragment: FragmentState<'_>,
) -> RenderPipeline {
    build_variant(
        device,
        layout,
        buffers,
        fragment,
        ObjectState {
            key,
            samples: target.0,
            front_face: target.1,
            depth_write: true,
            vertex_entry: shader::VERTEX_ENTRY_POINT,
        },
    )
}

#[derive(Clone, Copy)]
pub(super) struct ObjectState {
    key: PipelineKey,
    samples: u32,
    front_face: FrontFace,
    depth_write: bool,
    vertex_entry: &'static str,
}

pub(super) fn build_variant(
    device: &Device,
    layout: &PipelineLayout,
    buffers: &[Option<VertexBufferLayout<'_>>],
    fragment: FragmentState<'_>,
    state: ObjectState,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("environment"),
        layout: Some(layout),
        vertex: VertexState {
            module: fragment.module,
            entry_point: Some(state.vertex_entry),
            compilation_options: PipelineCompilationOptions::default(),
            buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: state.front_face,
            cull_mode: state.key.cull_mode(),
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(state.depth_write),
            depth_compare: Some(if state.depth_write {
                CompareFunction::Less
            } else {
                CompareFunction::LessEqual
            }),
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState {
            count: state.samples,
            ..Default::default()
        },
        fragment: Some(fragment),
        multiview_mask: None,
        cache: None,
    })
}

/// Builds the single pipeline the terrain surface draws through.
///
/// Terrain needs exactly one pipeline where the environment needs four, and the
/// reason is that both properties the key encodes are settled here rather than
/// varying. Terrain is never masked: the alpha channel of a terrain albedo is a
/// height field driving the splat blend, not coverage, so a discard would punch
/// holes in the ground rather than cut out foliage. And terrain is never
/// double-sided: it is a heightfield seen from above, so the back faces culled
/// here are faces of the world's underside that nothing can reach.
///
/// Everything shared with [`build`] is shared deliberately and must stay that
/// way. Terrain and the props standing on it write the same depth buffer in the
/// same pass, so a difference in depth format, comparison or write enable would
/// not shade wrongly - it would sort wrongly, and plants would sink into or
/// float above the ground they are placed on. The winding is shared for the
/// same reason the terrain mesh emits its corners in the order it does:
/// reversing it culls the visible surface and renders the world inside out.
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
            entry_point: Some(shader::TERRAIN_VERTEX_ENTRY_POINT),
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
            entry_point: Some(shader::TERRAIN_FRAGMENT_ENTRY_POINT),
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

/// Creates a depth attachment sized to a viewport.
///
/// The depth buffer is not reused across viewport sizes: it must match the
/// colour target exactly, and a stale one is a validation error rather than a
/// subtle artefact, so it is cheaper to recreate than to track.
#[must_use]
pub fn depth_texture(device: &Device, viewport: Viewport) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("scene-depth"),
        size: Extent3d {
            width: viewport.width,
            height: viewport.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

#[cfg(test)]
mod tests;

fn scene_shader(device: &Device, label: &str, source: &str) -> wgpu::ShaderModule {
    device.create_shader_module(ShaderModuleDescriptor {
        label: Some(label),
        source: ShaderSource::Wgsl(Cow::Borrowed(source)),
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

pub(crate) fn celestial_pipelines(
    device: &Device,
    scene: &ScenePipelines,
    physical: bool,
    separate: bool,
    object_exposure: bool,
) -> [RenderPipeline; 8] {
    celestial_specialization(
        device,
        scene,
        (physical, separate, object_exposure),
        ([0; 7], FrontFace::Ccw),
        worldrunner_render_plan::SceneCelestialLighting::NEUTRAL,
        false,
    )
}
pub(crate) fn celestial_specialization(
    device: &Device,
    scene: &ScenePipelines,
    composition: (bool, bool, bool),
    options: ([u32; 7], FrontFace),
    lighting: worldrunner_render_plan::SceneCelestialLighting,
    anchored: bool,
) -> [RenderPipeline; 8] {
    let mut constants = crate::sky::lighting::constants(options.0, lighting);
    specular::raster_constants(&mut constants, options.1);
    let source = ENVIRONMENT_WGSL.replace(
        "@location(0) world_position: vec3<f32>,",
        "@location(0) @interpolate(perspective, sample) world_position: vec3<f32>,",
    );
    let source = crate::sky::lighting::composition_source(source, composition);
    let source = crate::sky::lighting::source(source, lighting);
    let vertex = if anchored {
        crate::sky::meshes::VERTEX.replace(
            "instance.model_column_3.xyz+frame.camera_position.xyz",
            "instance.model_column_3.xyz",
        )
    } else {
        crate::sky::meshes::VERTEX.to_owned()
    };
    let source = format!("{source}\n{vertex}");
    let module = scene_shader(device, "celestial mesh lighting", &source);
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("celestial meshes"),
        bind_group_layouts: &[Some(scene.frame_layout()), Some(scene.material_layout())],
        immediate_size: 0,
    });
    let vertices = vertex_attributes();
    let instances = instance_attributes();
    let buffers = vertex_buffers(&vertices, &instances);
    let targets = color_targets(wgpu::TextureFormat::Rgba16Float);
    PipelineKey::ALL.map(|key| {
        build_variant(
            device,
            &layout,
            &buffers,
            FragmentState {
                module: &module,
                entry_point: Some(if key.masked {
                    "fs_celestial_masked"
                } else {
                    "fs_celestial_opaque"
                }),
                compilation_options: PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                targets: &targets,
            },
            ObjectState {
                key,
                samples: scene.samples,
                front_face: options.1,
                depth_write: true,
                vertex_entry: "vs_celestial",
            },
        )
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
    } else {
        color_targets(format).to_vec()
    }
}
