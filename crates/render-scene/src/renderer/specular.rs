use super::ScenePipelines;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(crate) fn unlit_source(source: &str) -> std::borrow::Cow<'_, str> {
    super::DepthMode::Independent.vertex_source(source)
}

pub(super) fn raster_constants(
    constants: &mut Vec<(&'static str, f64)>,
    front_face: wgpu::FrontFace,
) {
    if front_face == wgpu::FrontFace::Cw {
        constants.push(("SCENE_RASTER_MIRRORED", 1.0));
    }
}

pub(crate) fn constants(options: [u32; 7]) -> [(&'static str, f64); 7] {
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

type RasterKey = ([u32; 7], u8, super::DepthMode);

/// What a set of pipelines was built from, for the frozen terrain path to rebuild.
#[cfg(test)]
pub(super) struct Parts<'a> {
    pub object_source: &'a str,
    pub terrain_source: &'a str,
    pub shadow: Option<&'a wgpu::BindGroupLayout>,
    pub screen: Option<&'a wgpu::BindGroupLayout>,
    pub options: ([u32; 7], wgpu::FrontFace),
}

#[derive(Clone, Debug)]
pub(super) struct Specialization {
    device: wgpu::Device,
    source: Rc<str>,
    unlit: Rc<str>,
    terrain: Rc<str>,
    shadow: Option<wgpu::BindGroupLayout>,
    screen: Option<wgpu::BindGroupLayout>,
    options: ([u32; 7], wgpu::FrontFace, super::DepthMode),
    cache: Rc<RefCell<BTreeMap<RasterKey, ScenePipelines>>>,
    gather: Rc<RefCell<Option<ScenePipelines>>>,
    /// The program runtime every sibling of these pipelines shares, so a terrain pipeline
    /// compiles once per context however many scenes and specializations draw with it.
    family: crate::terrain::program::Family,
    receivers:
        Rc<RefCell<BTreeMap<crate::scene_metadata::receiver::ReceiverLayout, ScenePipelines>>>,
}
impl Specialization {
    pub fn new(
        device: &wgpu::Device,
        (source, terrain): (&str, &str),
        (shadow, screen): (
            Option<&wgpu::BindGroupLayout>,
            Option<&wgpu::BindGroupLayout>,
        ),
        options: ([u32; 7], wgpu::FrontFace, super::DepthMode),
        family: &crate::terrain::program::Family,
    ) -> Self {
        Self {
            device: device.clone(),
            source: source.into(),
            unlit: super::unlit_source(source).as_ref().into(),
            terrain: terrain.into(),
            shadow: shadow.cloned(),
            screen: screen.cloned(),
            options,
            cache: Rc::default(),
            gather: Rc::default(),
            family: family.clone(),
            receivers: Rc::default(),
        }
    }
}
impl Specialization {
    pub(super) fn unlit_source(&self) -> &str {
        &self.unlit
    }

    /// The object source, the terrain source, the lent shadow and screen layouts and the
    /// options these pipelines were built with.
    #[cfg(test)]
    pub(super) fn parts(&self) -> Parts<'_> {
        Parts {
            object_source: &self.source,
            terrain_source: &self.terrain,
            shadow: self.shadow.as_ref(),
            screen: self.screen.as_ref(),
            options: (self.options.0, self.options.1),
        }
    }
}

impl ScenePipelines {
    pub(crate) fn for_receiver_gradients(
        &self,
        receiver: crate::scene_metadata::receiver::ReceiverLayout,
    ) -> Self {
        let spec = &self.specialization;
        spec.receivers
            .borrow_mut()
            .entry(receiver)
            .or_insert_with(|| {
                let source =
                    crate::scene_metadata::receiver::consumer_source(&spec.source, receiver, false);
                let terrain =
                    crate::scene_metadata::receiver::consumer_source(&spec.terrain, receiver, true);
                let layout = crate::scene_metadata::receiver::shadow_layout(&spec.device);
                let mut selected = Self::with_constants(
                    &spec.device,
                    (self.color_format, self.samples),
                    (&source, &terrain),
                    (Some(&layout), spec.screen.as_ref()),
                    (spec.options, &spec.family),
                );
                selected.ssao = self.ssao;
                selected
            })
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn for_receiver_reference(
        &self,
        reference: crate::scene_metadata::receiver::ReceiverReference,
        receiver: crate::scene_metadata::receiver::ReceiverLayout,
    ) -> Self {
        let spec = &self.specialization;
        let sources = [&spec.source, &spec.terrain].map(|source| {
            assert!(!source.contains("scene_receiver_load("));
            assert!(source.contains("dpdx(position) + dpdx(normal) * shadow.bias.y"));
            if reference.probe() {
                let source = crate::scene_metadata::receiver::probe_source(source);
                if matches!(
                    reference,
                    crate::scene_metadata::receiver::ReceiverReference::BlackShared
                ) {
                    source.replace(
                        "scene_probe_dx = receiver_dx; scene_probe_dy = receiver_dy;",
                        "scene_probe_dx = vec3<f32>(0.0); scene_probe_dy = vec3<f32>(0.0);",
                    )
                } else if matches!(
                    reference,
                    crate::scene_metadata::receiver::ReceiverReference::OffsetProbeReuse
                        | crate::scene_metadata::receiver::ReceiverReference::StoredOffsetProbeReuse
                ) {
                    source.replace(
                        "scene_probe_dx = receiver_dx;",
                        "scene_probe_dx = receiver_dx + vec3<f32>(0.25, 0.5, 1.0);",
                    )
                } else {
                    source
                }
            } else {
                source.to_string()
            }
        });
        let layout = crate::scene_metadata::receiver::shadow_layout(&spec.device);
        let mut selected = Self::with_constants(
            &spec.device,
            (self.color_format, self.samples),
            (&sources[0], &sources[1]),
            (Some(&layout), spec.screen.as_ref()),
            (spec.options, &spec.family),
        );
        selected.ssao = self.ssao;
        if reference.local() {
            selected
        } else {
            let selected = selected.for_receiver_gradients(receiver);
            if matches!(
                reference,
                crate::scene_metadata::receiver::ReceiverReference::LateCardRejection
            ) {
                selected.with_late_card_rejection()
            } else {
                selected
            }
        }
    }

    #[cfg(test)]
    fn with_late_card_rejection(&self) -> Self {
        let spec = &self.specialization;
        let source = crate::scene_metadata::receiver::late_card_source(&spec.source);
        let mut selected = Self::with_constants(
            &spec.device,
            (self.color_format, self.samples),
            (&source, &spec.terrain),
            (spec.shadow.as_ref(), spec.screen.as_ref()),
            (spec.options, &spec.family),
        );
        selected.ssao = self.ssao;
        selected
    }

    pub(crate) fn for_depth(&self, depth: super::DepthMode) -> Self {
        if depth == self.depth {
            return self.clone();
        }
        assert!(depth == super::DepthMode::Independent || self.samples == 1);
        let spec = &self.specialization;
        let options = (spec.options.0, spec.options.1, depth);
        spec.cache
            .borrow_mut()
            .entry((options.0, u8::from(options.1 == wgpu::FrontFace::Cw), depth))
            .or_insert_with(|| {
                let mut selected = Self::with_constants(
                    &spec.device,
                    (self.color_format, self.samples),
                    (&spec.source, &spec.terrain),
                    (spec.shadow.as_ref(), spec.screen.as_ref()),
                    (options, &spec.family),
                );
                selected.ssao = self.ssao;
                selected
            })
            .clone()
    }

    pub(crate) fn has_ao_receiver(&self) -> bool {
        self.specialization.screen.is_some()
    }

    pub(crate) fn for_receiver(
        &self,
        receiver: worldrunner_render_plan::SceneSsaoReceiver,
    ) -> Self {
        let spec = &self.specialization;
        if !crate::ssao::receiver::gathers(receiver)
            || !spec.source.contains("fn screen_ao(")
            || spec.source.contains("fn receiver_gather(")
        {
            return self.clone();
        }
        spec.gather
            .borrow_mut()
            .get_or_insert_with(|| {
                let source = crate::ssao::receiver::source(&spec.source, receiver);
                let terrain = crate::ssao::receiver::source(&spec.terrain, receiver);
                let layout = if spec.source.contains("fn forward_reflection") {
                    crate::ssr::forward::receiver_layout(&spec.device, receiver)
                } else {
                    crate::ssao::receiver::layout(&spec.device)
                };
                let mut selected = Self::with_constants(
                    &spec.device,
                    (self.color_format, self.samples),
                    (&source, &terrain),
                    (spec.shadow.as_ref(), Some(&layout)),
                    (spec.options, &spec.family),
                );
                selected.ssao = self.ssao;
                selected
            })
            .clone()
    }

    pub(crate) fn sky_targets(&self) -> (bool, bool, super::DepthMode) {
        (
            self.writes_velocity(),
            self.specialization
                .source
                .contains("struct ReflectionOutput"),
            if self.depth == super::DepthMode::Independent {
                self.depth
            } else {
                super::DepthMode::Shared
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn for_convention(
        &self,
        convention: worldrunner_render_plan::SceneSpecularConvention,
    ) -> Self {
        self.for_raster(convention, wgpu::FrontFace::Ccw)
    }

    pub(crate) fn for_raster(
        &self,
        convention: worldrunner_render_plan::SceneSpecularConvention,
        front_face: wgpu::FrontFace,
    ) -> Self {
        let options = (convention.shader_options(), front_face, self.depth);
        let spec = &self.specialization;
        if options == spec.options {
            return self.clone();
        }
        spec.cache
            .borrow_mut()
            .entry((
                options.0,
                u8::from(front_face == wgpu::FrontFace::Cw),
                options.2,
            ))
            .or_insert_with(|| {
                let mut selected = Self::with_constants(
                    &spec.device,
                    (self.color_format, self.samples),
                    (&spec.source, &spec.terrain),
                    (spec.shadow.as_ref(), spec.screen.as_ref()),
                    (options, &spec.family),
                );
                selected.ssao = self.ssao;
                selected
            })
            .clone()
    }
}

impl ScenePipelines {
    /// The fixed unlit-alpha pipeline these pipelines carried before dynamic quads drew through
    /// the program runtime, rebuilt from the same shader source, layouts and targets. It is the
    /// reference the byte-equality captures compare the runtime's output with.
    #[cfg(test)]
    pub(crate) fn legacy_unlit_alpha(&self) -> wgpu::RenderPipeline {
        let spec = &self.specialization;
        let module = super::scene_shader(&spec.device, "environment", spec.unlit_source());
        let groups = super::pipeline_groups(
            &self.frame_layout,
            &self.material_layout,
            spec.shadow.as_ref(),
            spec.screen.as_ref(),
        );
        let layout = super::scene_layout(&spec.device, "environment", &groups);
        let vertices = crate::geometry::vertex_attributes();
        let instances = crate::geometry::instance_attributes();
        let buffers = super::vertex_buffers(&vertices, &instances);
        let targets = super::source_targets(&spec.source, self.color_format);
        super::unlit::build(
            &spec.device,
            (self.color_format, self.samples),
            &layout,
            &buffers,
            &module,
            &targets,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_plan::{
        SceneRoughnessRemap, SceneSpecularConvention, SceneSpecularOcclusion,
        SceneSpecularVisibility,
    };

    #[test]
    fn convention_cache_reuses_each_selection_and_preserves_default_pipelines() {
        let _session = crate::test_support::SceneTestSession::acquire();
        let context = crate::device::GpuContext::shared_for_test().unwrap();
        let pipelines = ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba16Float);
        let key = crate::pipeline::PipelineKey::ALL[0];
        for roughness in [
            SceneRoughnessRemap::Perceptual045,
            SceneRoughnessRemap::Perceptual04,
            SceneRoughnessRemap::LinearAlpha,
        ] {
            for visibility in [
                SceneSpecularVisibility::SmithCorrelated,
                SceneSpecularVisibility::SchlickSmith,
            ] {
                for occlusion in [
                    SceneSpecularOcclusion::AmbientOnly,
                    SceneSpecularOcclusion::RoughnessAware,
                ] {
                    let convention = SceneSpecularConvention {
                        roughness,
                        visibility,
                        occlusion,
                        ..Default::default()
                    };
                    let first = pipelines.for_convention(convention);
                    let again = pipelines.for_convention(convention);
                    assert_eq!(first.get(key), again.get(key));
                    if convention == SceneSpecularConvention::default() {
                        assert_eq!(first.get(key), pipelines.get(key));
                    } else {
                        assert_ne!(first.get(key), pipelines.get(key));
                    }
                }
            }
        }
        assert_eq!(pipelines.specialization.cache.borrow().len(), 11);
    }
}

#[cfg(test)]
#[test]
fn disk_radius_is_part_of_existing_pipeline_cache_key() {
    use worldrunner_render_plan::{SceneSpecularConvention, SceneSurfaceDisk};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let pipelines = ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba16Float);
    let convention = |radius| SceneSpecularConvention {
        surface_disk: Some(
            SceneSurfaceDisk::new(1_000_000, 1_000_000)
                .unwrap()
                .with_angular_radius_microradians(radius)
                .unwrap(),
        ),
        ..Default::default()
    };
    let key = crate::pipeline::PipelineKey::ALL[0];
    let default = pipelines.for_convention(convention(2700));
    let wide = pipelines.for_convention(convention(17_453));
    assert_ne!(default.get(key), wide.get(key));
    assert_eq!(
        wide.get(key),
        pipelines.for_convention(convention(17_453)).get(key)
    );
    assert_eq!(pipelines.specialization.cache.borrow().len(), 2);
}

#[cfg(test)]
#[test]
fn winding_uses_existing_specialization_cache_without_changing_default_pipelines() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let pipelines = ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba16Float);
    let convention = worldrunner_render_plan::SceneSpecularConvention::default();
    let key = crate::pipeline::PipelineKey::ALL[0];
    let right = pipelines.for_raster(convention, wgpu::FrontFace::Ccw);
    let left = pipelines.for_raster(convention, wgpu::FrontFace::Cw);
    let again = pipelines.for_raster(convention, wgpu::FrontFace::Cw);
    assert_eq!(right.get(key), pipelines.get(key));
    assert_eq!(left.get(key), again.get(key));
    assert_ne!(left.get(key), right.get(key));
    assert_eq!(pipelines.specialization.cache.borrow().len(), 1);
}
