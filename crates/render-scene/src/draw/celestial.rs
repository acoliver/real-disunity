use super::{GpuMesh, ScenePipelines};
use wgpu::util::DeviceExt;
use worldrunner_render_plan::RenderScene;

pub(super) fn upload(
    device: &wgpu::Device,
    scene: &RenderScene,
    pipelines: &ScenePipelines,
    meshes: &[GpuMesh],
    materials: &[wgpu::BindGroup],
) -> Option<crate::sky::meshes::Meshes> {
    let settings = scene.effects()?.atmosphere()?;
    if settings.meshes.is_empty() {
        return None;
    }
    let draws = settings
        .meshes
        .iter()
        .map(|settings| {
            let lod = &meshes[settings.mesh.raw() as usize].lods[0];
            let material = &scene.resources().materials()[settings.material.raw() as usize];
            let mut bytes =
                crate::geometry::pack_model_instance(crate::sky::meshes::model(*settings));
            crate::geometry::tint_instance(&mut bytes, settings.tint);
            let instance = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("celestial mesh pose"),
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
            crate::sky::meshes::MeshDraw {
                lighting: settings.lighting,
                anchored: settings.anchor_millimeters.is_some(),
                vertices: lod.vertices.clone(),
                indices: lod.indices.clone(),
                count: lod.index_count,
                material: materials[settings.material.raw() as usize].clone(),
                instance,
                key: crate::pipeline::PipelineKey::from_material(material),
            }
        })
        .collect();
    Some(crate::sky::meshes::Meshes::new(
        device,
        pipelines,
        draws,
        settings.scattering.is_some_and(|s| s.compose_celestial),
    ))
}

impl super::SceneGpu {
    pub(crate) fn plan_celestial(
        &self,
        device: &wgpu::Device,
        size: [u32; 2],
        passes: &mut Vec<worldrunner_render_post::PassDeclaration>,
        views: &mut Vec<worldrunner_render_post::AttachmentView>,
        declaration: &mut worldrunner_render_post::FeatureDeclaration,
    ) {
        if let Some(meshes) = &self.celestial {
            meshes.plan(device, size, passes, views, declaration);
        }
    }
    pub(crate) fn draw_celestial(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(meshes) = &self.celestial {
            meshes.draw_geometry(
                pass,
                &self.drawing_frame_group(),
                self.drawing_uniform().specular_convention,
            );
        }
    }
}
