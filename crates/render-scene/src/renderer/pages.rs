//! The fixed terrain pipelines, built when a draw first needs one.
//!
//! Terrain draws through the program runtime in every forward context, so these pipelines only
//! serve the contexts that have not moved (the metadata prepass and the capture terms) and the
//! frozen path's comparisons. Compiling four terrain pipelines in every context a scene is
//! specialized for was the main cost of constructing a `ScenePipelines`.

use std::rc::Rc;
use std::sync::OnceLock;

use super::{
    BindGroupLayout, ColorTargetState, Device, PipelineLayoutDescriptor, RenderPipeline,
    build_terrain,
};
use crate::terrain::program::source::{cached_source, layered_source};

/// What every fixed terrain pipeline of a `ScenePipelines` is built from.
#[derive(Debug)]
pub(super) struct Inputs {
    device: Device,
    /// The pipeline layout's groups, with group one the terrain's own.
    groups: Vec<Option<BindGroupLayout>>,
    source: String,
    targets: Vec<Option<ColorTargetState>>,
    samples: u32,
    constants: Vec<(&'static str, f64)>,
    front_face: wgpu::FrontFace,
    depth: super::DepthMode,
}

/// The pipelines of one surface family: the palette (or layered) draw and the cached-page draw,
/// each built on first use and shared by every clone.
#[derive(Clone, Debug)]
pub(crate) struct Pair(Rc<Lazy>);

#[derive(Debug)]
struct Lazy {
    inputs: Rc<Inputs>,
    layered: bool,
    layout: BindGroupLayout,
    cached_layout: BindGroupLayout,
    terrain: OnceLock<RenderPipeline>,
    cached: OnceLock<RenderPipeline>,
}

impl Pair {
    fn new(inputs: &Rc<Inputs>, layout: BindGroupLayout, layered: bool) -> Self {
        let cached_layout = crate::upload::terrain_layout(&inputs.device, true, layered);
        Self(Rc::new(Lazy {
            inputs: inputs.clone(),
            layered,
            layout,
            cached_layout,
            terrain: OnceLock::new(),
            cached: OnceLock::new(),
        }))
    }

    /// The layout of the terrain's own group.
    pub(crate) fn layout(&self) -> &BindGroupLayout {
        &self.0.layout
    }

    /// The layout of the group of a cached-page draw.
    pub(crate) fn cached_layout(&self) -> &BindGroupLayout {
        &self.0.cached_layout
    }

    fn source(&self) -> String {
        let inputs = &self.0.inputs;
        let source = if self.0.layered {
            layered_source(&inputs.source)
        } else {
            inputs.source.clone()
        };
        include_str!("../terrain/pages/footprint.wgsl").to_owned() + &source
    }

    fn build(&self, layout: &BindGroupLayout, source: &str, label: &str) -> RenderPipeline {
        let inputs = &self.0.inputs;
        let mut groups: Vec<Option<&BindGroupLayout>> =
            inputs.groups.iter().map(Option::as_ref).collect();
        groups[1] = Some(layout);
        let pipeline_layout = inputs
            .device
            .create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &groups,
                immediate_size: 0,
            });
        let vertices = crate::geometry::vertex_attributes();
        let instances = crate::geometry::instance_attributes();
        let buffers = super::vertex_buffers(&vertices, &instances);
        build_terrain(
            &inputs.device,
            &pipeline_layout,
            source,
            &buffers,
            &inputs.targets,
            (
                inputs.samples,
                &inputs.constants,
                inputs.front_face,
                inputs.depth,
            ),
        )
    }

    /// The pipeline that blends the palette (or the layered sources) through the type grid.
    pub(crate) fn terrain(&self) -> &RenderPipeline {
        self.0
            .terrain
            .get_or_init(|| self.build(&self.0.layout, &self.source(), "terrain"))
    }

    /// The pipeline that reads cached material pages.
    pub(crate) fn cached(&self) -> &RenderPipeline {
        self.0.cached.get_or_init(|| {
            self.build(
                &self.0.cached_layout,
                &cached_source(&self.source()),
                "cached terrain",
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn is_built(&self) -> bool {
        self.0.terrain.get().is_some() || self.0.cached.get().is_some()
    }
}

/// The fixed pipelines of a terrain, for the palette and, when the device supports it, the
/// layered sources.
pub(super) fn build_terrain_set(
    device: &Device,
    groups: &[Option<&BindGroupLayout>],
    source: &str,
    targets: &[Option<ColorTargetState>],
    target: (
        u32,
        &[(&'static str, f64)],
        wgpu::FrontFace,
        super::DepthMode,
    ),
) -> (Pair, Option<Pair>) {
    let inputs = Rc::new(Inputs {
        device: device.clone(),
        groups: groups.iter().map(|group| group.cloned()).collect(),
        source: source.to_owned(),
        targets: targets.to_vec(),
        samples: target.0,
        constants: target.1.to_vec(),
        front_face: target.2,
        depth: target.3,
    });
    let plain = Pair::new(&inputs, groups[1].expect("terrain group").clone(), false);
    let layered = crate::terrain::pages::layered_gpu::supported(&device.limits()).then(|| {
        let layout = crate::upload::terrain_layout(device, false, true);
        Pair::new(&inputs, layout, true)
    });
    (plain, layered)
}

#[cfg(test)]
pub(crate) fn test_sources(source: &str) -> [String; 4] {
    let prefix = include_str!("../terrain/pages/footprint.wgsl");
    let plain = format!("{prefix}{source}");
    let layered = format!("{prefix}{}", layered_source(source));
    [
        plain.clone(),
        cached_source(&plain),
        layered.clone(),
        cached_source(&layered),
    ]
}
