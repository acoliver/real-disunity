use super::{PipelineKey, build, vertex_buffers};
use std::{ops::Index, rc::Rc, sync::OnceLock};

/// Material variants are compiled once, only when a draw needs them.
#[derive(Clone, Debug)]
pub(crate) struct ObjectPipelines(Rc<Objects>);

#[derive(Debug)]
struct Objects {
    device: wgpu::Device,
    layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    targets: Vec<Option<wgpu::ColorTargetState>>,
    previous_transforms: bool,
    samples: u32,
    front_face: wgpu::FrontFace,
    constants: Vec<(String, f64)>,
    pipelines: [OnceLock<wgpu::RenderPipeline>; 16],
}

impl ObjectPipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        targets: &[Option<wgpu::ColorTargetState>],
        module: &wgpu::ShaderModule,
        target: (u32, &[(&str, f64)], wgpu::FrontFace),
        previous_transforms: bool,
    ) -> Self {
        Self(Rc::new(Objects {
            device: device.clone(),
            layout: layout.clone(),
            module: module.clone(),
            targets: targets.to_vec(),
            previous_transforms,
            samples: target.0,
            front_face: target.2,
            constants: target
                .1
                .iter()
                .map(|(name, value)| (String::from(*name), *value))
                .collect(),
            pipelines: std::array::from_fn(|_| OnceLock::new()),
        }))
    }
}

impl Index<usize> for ObjectPipelines {
    type Output = wgpu::RenderPipeline;

    fn index(&self, index: usize) -> &Self::Output {
        self.0.pipelines[index].get_or_init(|| {
            let state = &self.0;
            let key = PipelineKey::ALL[index];
            let vertices = crate::geometry::vertex_attributes();
            let instances = crate::geometry::instance_attributes();
            let previous = crate::geometry::previous_instance_attributes();
            let mut buffers = vertex_buffers(&vertices, &instances).to_vec();
            if state.previous_transforms {
                buffers.push(Some(crate::renderer::previous_instance_buffer(&previous)));
            }
            let mut constants: Vec<_> = state
                .constants
                .iter()
                .map(|(name, value)| (name.as_str(), *value))
                .collect();
            constants.push((
                "SCENE_ALPHA_TO_COVERAGE",
                f64::from(key.alpha_to_coverage(state.samples)),
            ));
            let entry = match (key.relief, key.masked) {
                (true, true) => "fs_relief_masked",
                (true, false) => "fs_relief_opaque",
                (false, masked) => crate::shader::fragment_entry_point(masked),
            };
            build(
                &state.device,
                key,
                &state.layout,
                &buffers,
                (state.samples, state.front_face),
                wgpu::FragmentState {
                    module: &state.module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets: &state.targets,
                },
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_unused_material_variants_are_not_compiled_and_clones_share_variants() {
        let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
        let scene =
            super::super::ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba8Unorm);
        let objects = scene.object_pipelines();
        assert!(objects.0.pipelines.iter().all(|slot| slot.get().is_none()));
        let clone = objects.clone();
        for key in PipelineKey::ALL {
            assert_eq!(objects[key.index()], clone[key.index()]);
        }
        assert!(objects.0.pipelines.iter().all(|slot| slot.get().is_some()));
    }
}
