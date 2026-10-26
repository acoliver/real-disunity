//! Camera-relative celestial meshes reuse scene vertices, materials and lighting.
use glam::{Mat4, Quat, Vec3};
use worldrunner_render_plan::{SceneCelestialMesh, SceneRotation};

pub(crate) const VERTEX: &str = r"
@vertex fn vs_celestial(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var absolute=instance;
    absolute.model_column_3=vec4(instance.model_column_3.xyz+frame.camera_position.xyz,1.0);
    var out=material_vertex(vertex,absolute);
    let clip=out.clip_position;
    out.clip_position=vec4(clip.xy,0.5*clip.w,clip.w);
    return out;
}
struct CelestialFragment {
    @location(0) color:vec4<f32>,
    @builtin(frag_depth) depth:f32,
}
fn celestial_fragment(input:VertexOutput,front_facing:bool,masked:bool)->CelestialFragment {
    let shaded=shade(input,front_facing);
    let clip=frame.view_projection*vec4(input.world_position,1.0);
    // Clip in the original camera space, but retain distant background surfaces.
    if clip.z<0.0 || clip.w<=0.0 { discard; }
    if masked && !scene_alpha_covered(shaded.a,input) { discard; }
    scene_apply_coverage(input);
    let depth=clip.z/clip.w;
    var out:CelestialFragment;
    out.color=vec4(shaded.rgb,1.0);
    // Evaluate after interpolation so intersecting triangles keep their real ordering.
    out.depth=depth/(1.0+depth);
    return out;
}
@fragment fn fs_celestial_opaque(input:VertexOutput,@builtin(front_facing) front:bool)->CelestialFragment {
    return celestial_fragment(input,front,false);
}
@fragment fn fs_celestial_masked(input:VertexOutput,@builtin(front_facing) front:bool)->CelestialFragment {
    return celestial_fragment(input,front,true);
}
";
#[expect(
    clippy::cast_precision_loss,
    reason = "scene quantized directions, rotations and distances"
)]
pub(crate) fn model(settings: SceneCelestialMesh) -> Mat4 {
    let direction = Vec3::from_array(
        settings
            .direction
            .components_micro()
            .map(|v| v as f32 / 1e6),
    )
    .normalize();
    let distance = settings.distance_millimeters as f32 / 1000.0;
    let radius = (settings.angular_radius_millidegrees as f32 / 1000.0)
        .to_radians()
        .sin()
        * distance;
    let scale = radius / (settings.model_radius_millimeters as f32 / 1000.0);
    let SceneRotation { x, y, z, w } = settings.rotation;
    let rotation = Quat::from_xyzw(
        x as f32 / 1e6,
        y as f32 / 1e6,
        z as f32 / 1e6,
        w as f32 / 1e6,
    )
    .normalize();
    Mat4::from_scale_rotation_translation(Vec3::splat(scale), rotation, direction * distance)
}
#[derive(Debug)]
pub(crate) struct MeshDraw {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub count: u32,
    pub material: wgpu::BindGroup,
    pub instance: wgpu::Buffer,
    pub key: crate::pipeline::PipelineKey,
}
#[derive(Debug)]
pub(crate) struct Meshes {
    pipelines: [wgpu::RenderPipeline; 4],
    draws: Vec<MeshDraw>,
    composite: wgpu::RenderPipeline,
    samples: u32,
    layout: wgpu::BindGroupLayout,
    target: std::cell::RefCell<Option<super::mesh_target::Target>>,
}
impl Meshes {
    pub fn new(
        device: &wgpu::Device,
        pipelines: &crate::renderer::ScenePipelines,
        draws: Vec<MeshDraw>,
        physical: bool,
    ) -> Self {
        let (composite, layout) = super::mesh_target::pipeline(device, pipelines);
        Self {
            composite,
            samples: pipelines.samples,
            layout,
            target: std::cell::RefCell::new(None),
            pipelines: crate::renderer::celestial_pipelines(device, pipelines, physical),
            draws,
        }
    }
    pub fn plan(
        &self,
        device: &wgpu::Device,
        size: [u32; 2],
        passes: &mut Vec<worldrunner_render_post::PassDeclaration>,
        views: &mut Vec<worldrunner_render_post::AttachmentView>,
        declaration: &mut worldrunner_render_post::FeatureDeclaration,
    ) {
        let mut target = self.target.borrow_mut();
        if target.as_ref().is_none_or(|t| t.color.size() != size) {
            *target = Some(super::mesh_target::Target::new(
                device,
                size,
                self.samples,
                &self.layout,
            ));
        }
        target
            .as_ref()
            .expect("allocated visibility")
            .schedule(passes, views, declaration);
    }
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let target = self.target.borrow();
        let target = target
            .as_ref()
            .expect("celestial meshes require scheduled visibility before scene draw");
        pass.set_pipeline(&self.composite);
        pass.set_bind_group(2, &target.group, &[]);
        pass.draw(0..3, 0..1);
    }
    pub fn draw_geometry(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        for draw in &self.draws {
            pass.set_pipeline(&self.pipelines[draw.key.index()]);
            pass.set_bind_group(0, frame, &[]);
            pass.set_bind_group(1, &draw.material, &[]);
            pass.set_vertex_buffer(0, draw.vertices.slice(..));
            pass.set_vertex_buffer(1, draw.instance.slice(..));
            pass.set_index_buffer(draw.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..draw.count, 0, 0..1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneDirection};
    fn settings() -> SceneCelestialMesh {
        SceneCelestialMesh {
            direction: SceneDirection::new([0, 1_000_000, 0]).unwrap(),
            angular_radius_millidegrees: 30_000,
            distance_millimeters: 100_000,
            model_radius_millimeters: 1000,
            rotation: SceneRotation::IDENTITY,
            mesh: MeshHandle::from_raw(0),
            material: MaterialHandle::from_raw(0),
        }
    }
    #[test]
    fn mesh_radius_is_angular_and_pose_is_camera_relative() {
        let model = model(settings());
        assert!((model.transform_point3(Vec3::ZERO) - Vec3::Y * 100.0).length() < 1e-5);
        assert!((model.transform_vector3(Vec3::X).length() - 50.0).abs() < 1e-5);
        let mut rotated = settings();
        rotated.rotation = SceneRotation {
            x: 0,
            y: 0,
            z: 707_107,
            w: 707_107,
        };
        assert!(
            (super::model(rotated).transform_vector3(Vec3::X) - Vec3::Y * 50.0).length() < 1e-4
        );
        rotated.model_radius_millimeters = 0;
        assert!(rotated.validate().is_err());
    }
    #[test]
    fn mesh_pipelines_share_the_resident_material_layout() {
        let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
        let scope = context
            .device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let scene = crate::renderer::ScenePipelines::new(
            context.device(),
            wgpu::TextureFormat::Rgba16Float,
        );
        let _ = crate::renderer::celestial_pipelines(context.device(), &scene, false);
        assert!(pollster::block_on(scope.pop()).is_none());
    }
}
