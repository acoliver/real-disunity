use super::ScenePipelines;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(crate) fn constants(options: [u32; 3]) -> [(&'static str, f64); 3] {
    [
        ("SCENE_ROUGHNESS_REMAP", f64::from(options[0])),
        ("SCENE_SPECULAR_VISIBILITY", f64::from(options[1])),
        ("SCENE_SPECULAR_OCCLUSION", f64::from(options[2])),
    ]
}

#[derive(Clone, Debug)]
pub(super) struct Specialization {
    device: wgpu::Device,
    source: Rc<str>,
    terrain: Rc<str>,
    shadow: Option<wgpu::BindGroupLayout>,
    screen: Option<wgpu::BindGroupLayout>,
    options: [u32; 3],
    cache: Rc<RefCell<BTreeMap<[u32; 3], ScenePipelines>>>,
}
impl Specialization {
    pub fn new(
        device: &wgpu::Device,
        source: &str,
        terrain: &str,
        shadow: Option<&wgpu::BindGroupLayout>,
        screen: Option<&wgpu::BindGroupLayout>,
        options: [u32; 3],
    ) -> Self {
        Self {
            device: device.clone(),
            source: source.into(),
            terrain: terrain.into(),
            shadow: shadow.cloned(),
            screen: screen.cloned(),
            options,
            cache: Rc::default(),
        }
    }
}
impl ScenePipelines {
    pub(crate) fn for_convention(
        &self,
        convention: worldrunner_render_plan::SceneSpecularConvention,
    ) -> Self {
        let options = convention.shader_options();
        let spec = &self.specialization;
        if options == spec.options {
            return self.clone();
        }
        spec.cache
            .borrow_mut()
            .entry(options)
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
