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
#[derive(Debug)]
pub struct ScenePipelines {
    frame_layout: BindGroupLayout,
    material_layout: BindGroupLayout,
    terrain_layout: BindGroupLayout,
    pipelines: [RenderPipeline; 4],
    terrain: RenderPipeline,
}

impl ScenePipelines {
    /// Builds every pipeline variant for one colour target format.
    ///
    /// All four are built up front rather than on first use. The set is closed
    /// and tiny, and building eagerly means a shader or layout fault is a
    /// startup failure rather than a stall the first time the camera turns to
    /// face an unusual material.
    #[must_use]
    pub fn new(device: &Device, color_format: TextureFormat) -> Self {
        Self::with_environment(device, color_format, ENVIRONMENT_WGSL, None)
    }

    pub(crate) fn with_environment(
        device: &Device,
        color_format: TextureFormat,
        source: &str,
        shadow_layout: Option<&BindGroupLayout>,
    ) -> Self {
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("environment"),
            source: ShaderSource::Wgsl(Cow::Borrowed(source)),
        });
        let frame_layout = frame_bind_group_layout(device);
        let material_layout = material_bind_group_layout(device);
        let terrain_layout = terrain_bind_group_layout(device);
        let mut groups = vec![Some(&frame_layout), Some(&material_layout)];
        if let Some(shadow_layout) = shadow_layout {
            groups.push(Some(shadow_layout));
        }
        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("environment"),
            bind_group_layouts: &groups,
            immediate_size: 0,
        });

        let vertex_attributes = vertex_attributes();
        let instance_attributes = instance_attributes();
        let buffers = [
            Some(VertexBufferLayout {
                array_stride: VERTEX_STRIDE,
                step_mode: VertexStepMode::Vertex,
                attributes: &vertex_attributes,
            }),
            Some(VertexBufferLayout {
                array_stride: INSTANCE_STRIDE,
                step_mode: VertexStepMode::Instance,
                attributes: &instance_attributes,
            }),
        ];
        let targets = [Some(ColorTargetState {
            format: color_format,
            blend: None,
            write_mask: ColorWrites::ALL,
        })];

        let pipelines = PipelineKey::ALL.map(|key| {
            build(
                device,
                key,
                &layout,
                &buffers,
                FragmentState {
                    module: &module,
                    entry_point: Some(shader::fragment_entry_point(key.masked)),
                    compilation_options: PipelineCompilationOptions::default(),
                    targets: &targets,
                },
            )
        });
        let terrain = build_terrain(device, &frame_layout, &terrain_layout, &buffers, &targets);

        Self {
            frame_layout,
            material_layout,
            terrain_layout,
            pipelines,
            terrain,
        }
    }

    pub(crate) const fn object_pipelines(&self) -> &[RenderPipeline; 4] {
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
    fragment: FragmentState<'_>,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("environment"),
        layout: Some(layout),
        vertex: VertexState {
            module: fragment.module,
            entry_point: Some(shader::VERTEX_ENTRY_POINT),
            compilation_options: PipelineCompilationOptions::default(),
            buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: key.cull_mode(),
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
        multisample: MultisampleState::default(),
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
    frame_layout: &BindGroupLayout,
    terrain_layout: &BindGroupLayout,
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
) -> RenderPipeline {
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("terrain"),
        source: ShaderSource::Wgsl(Cow::Borrowed(TERRAIN_WGSL)),
    });
    // Group 0 is the very same layout object the environment pipeline uses, so
    // the two pipeline layouts agree at that slot and the frame binding is not
    // invalidated when the pass switches between them. Substituting an
    // equivalent-but-separate layout here would work today and would make that
    // guarantee depend on two declarations staying in step.
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("terrain"),
        bind_group_layouts: &[Some(frame_layout), Some(terrain_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("terrain"),
        layout: Some(&layout),
        vertex: VertexState {
            module: &module,
            entry_point: Some(shader::TERRAIN_VERTEX_ENTRY_POINT),
            compilation_options: PipelineCompilationOptions::default(),
            buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
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
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &module,
            entry_point: Some(shader::TERRAIN_FRAGMENT_ENTRY_POINT),
            compilation_options: PipelineCompilationOptions::default(),
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
