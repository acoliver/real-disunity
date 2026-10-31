use super::ScenePipelines;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

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

type RasterKey = ([u32; 7], u8);

#[derive(Clone, Debug)]
pub(super) struct Specialization {
    device: wgpu::Device,
    source: Rc<str>,
    terrain: Rc<str>,
    shadow: Option<wgpu::BindGroupLayout>,
    screen: Option<wgpu::BindGroupLayout>,
    options: ([u32; 7], wgpu::FrontFace),
    cache: Rc<RefCell<BTreeMap<RasterKey, ScenePipelines>>>,
    gather: Rc<RefCell<Option<ScenePipelines>>>,
}
impl Specialization {
    pub fn new(
        device: &wgpu::Device,
        source: &str,
        terrain: &str,
        shadow: Option<&wgpu::BindGroupLayout>,
        screen: Option<&wgpu::BindGroupLayout>,
        options: ([u32; 7], wgpu::FrontFace),
    ) -> Self {
        Self {
            device: device.clone(),
            source: source.into(),
            terrain: terrain.into(),
            shadow: shadow.cloned(),
            screen: screen.cloned(),
            options,
            cache: Rc::default(),
            gather: Rc::default(),
        }
    }
}
impl ScenePipelines {
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
                    spec.shadow.as_ref(),
                    Some(&layout),
                    spec.options,
                );
                selected.ssao = self.ssao;
                selected
            })
            .clone()
    }

    pub(crate) fn sky_targets(&self) -> (bool, bool) {
        (
            self.writes_velocity(),
            self.specialization
                .source
                .contains("struct ReflectionOutput"),
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
        let options = (convention.shader_options(), front_face);
        let spec = &self.specialization;
        if options == spec.options {
            return self.clone();
        }
        spec.cache
            .borrow_mut()
            .entry((options.0, u8::from(front_face == wgpu::FrontFace::Cw)))
            .or_insert_with(|| {
                let mut selected = Self::with_constants(
                    &spec.device,
                    (self.color_format, self.samples),
                    (&spec.source, &spec.terrain),
                    spec.shadow.as_ref(),
                    spec.screen.as_ref(),
                    options,
                );
                selected.ssao = self.ssao;
                selected
            })
            .clone()
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
        let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
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
                    assert_eq!(first.terrain(), again.terrain());
                    assert_eq!(first.cached_terrain, again.cached_terrain);
                    if convention == SceneSpecularConvention::default() {
                        assert_eq!(first.get(key), pipelines.get(key));
                    } else {
                        assert_ne!(first.get(key), pipelines.get(key));
                        assert_ne!(first.terrain(), pipelines.terrain());
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
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
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
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let pipelines = ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba16Float);
    let convention = worldrunner_render_plan::SceneSpecularConvention::default();
    let key = crate::pipeline::PipelineKey::ALL[0];
    let right = pipelines.for_raster(convention, wgpu::FrontFace::Ccw);
    let left = pipelines.for_raster(convention, wgpu::FrontFace::Cw);
    let again = pipelines.for_raster(convention, wgpu::FrontFace::Cw);
    assert_eq!(right.get(key), pipelines.get(key));
    assert_eq!(left.get(key), again.get(key));
    assert_ne!(left.get(key), right.get(key));
    assert_ne!(left.terrain(), right.terrain());
    assert_eq!(left.terrain(), again.terrain());
    assert_ne!(left.cached_terrain, right.cached_terrain);
    assert_eq!(left.cached_terrain, again.cached_terrain);
    assert_eq!(pipelines.specialization.cache.borrow().len(), 1);
}
