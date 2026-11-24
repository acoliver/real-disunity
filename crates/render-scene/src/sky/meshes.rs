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
    let shaded=shade(input,front_facing,scene_relief_position(input));
    let coverage_alpha = scene_mask_alpha(shaded.a, input);
    let clip=frame.view_projection*vec4(input.world_position,1.0);
    // Clip in the original camera space, but retain distant background surfaces.
    if clip.z<0.0 || clip.w<=0.0 { discard; }
    if masked && !scene_alpha_covered(shaded.a,input) { discard; }
    scene_apply_coverage(input);
    let depth=clip.z/clip.w;
    var out:CelestialFragment;
    out.color=vec4(shaded.rgb,select(1.0, coverage_alpha, masked));
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
    Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        rotation,
        direction * distance
            + settings.anchor_millimeters.map_or(Vec3::ZERO, |p| {
                Vec3::from_array(p.map(|v| v as f32 / 1000.0))
            }),
    )
}
#[derive(Debug)]
pub(crate) struct MeshDraw {
    pub lighting: worldrunner_render_plan::SceneCelestialLighting,
    pub anchored: bool,
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    /// The level's first index in `indices` and the vertex its indices count from in `vertices`,
    /// which hold other levels as well.
    pub first_index: u32,
    pub base_vertex: i32,
    pub count: u32,
    /// The scene material the body draws with, whose prepared set the program path binds.
    pub material_index: usize,
    pub instance: wgpu::Buffer,
    pub key: crate::pipeline::PipelineKey,
}
/// The celestial programs of each selection: specular options, winding, body lighting and
/// anchoring. Each context makes the programs of the keys its bodies draw on first use.
type ContextCache =
    std::cell::RefCell<std::collections::BTreeMap<[u32; 12], crate::object::program::Context>>;

#[derive(Debug)]
pub(crate) struct Meshes {
    device: wgpu::Device,
    scene: crate::renderer::ScenePipelines,
    physical: bool,
    separate: bool,
    object_exposure: bool,
    contexts: ContextCache,
    draws: Vec<MeshDraw>,
    composites: std::cell::RefCell<
        std::collections::BTreeMap<
            (bool, bool, crate::renderer::depth::DepthMode),
            wgpu::RenderPipeline,
        >,
    >,
    samples: u32,
    layout: wgpu::BindGroupLayout,
    target: std::cell::RefCell<Option<super::mesh_target::Target>>,
    /// How `draw_geometry` records, for the captures that compare the live draw with no draw at
    /// all.
    #[cfg(test)]
    pub(crate) recorder: std::cell::Cell<Recorder>,
}
/// How a test has `draw_geometry` record the bodies.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Recorder {
    #[default]
    Live,
    Nothing,
}
impl Meshes {
    pub fn new(
        device: &wgpu::Device,
        pipelines: &crate::renderer::ScenePipelines,
        draws: Vec<MeshDraw>,
        physical: bool,
        separate: bool,
        object_exposure: bool,
    ) -> Self {
        let key = pipelines.sky_targets();
        let (composite, layout) = super::mesh_target::pipeline(device, pipelines, key);
        Self {
            composites: std::cell::RefCell::new(std::collections::BTreeMap::from([(
                key, composite,
            )])),
            device: device.clone(),
            scene: pipelines.clone(),
            physical,
            separate,
            object_exposure,
            contexts: std::cell::RefCell::default(),
            #[cfg(test)]
            recorder: std::cell::Cell::default(),
            samples: pipelines.samples,
            layout,
            target: std::cell::RefCell::new(None),
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
        self.prepare_target(device, size);
        self.target
            .borrow()
            .as_ref()
            .expect("allocated visibility")
            .schedule(passes, views, declaration);
    }
    fn prepare_target(&self, device: &wgpu::Device, size: [u32; 2]) {
        let mut target = self.target.borrow_mut();
        if target.as_ref().is_none_or(|t| t.color.size() != size) {
            *target = Some(super::mesh_target::Target::new(
                device,
                size,
                self.samples,
                &self.layout,
            ));
        }
    }
    pub fn capture_textures(&self, device: &wgpu::Device, size: [u32; 2]) -> [wgpu::Texture; 2] {
        self.prepare_target(device, size);
        let target = self.target.borrow();
        let target = target.as_ref().expect("allocated visibility");
        [
            target.color.texture().clone(),
            target.depth.texture().clone(),
        ]
    }
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        key: (bool, bool, crate::renderer::depth::DepthMode),
    ) {
        let target = self.target.borrow();
        let target = target
            .as_ref()
            .expect("celestial meshes require scheduled visibility before scene draw");
        // The immutable scene owns the colour format, sample count and frame layout.
        let mut composites = self.composites.borrow_mut();
        let composite = composites
            .entry(key)
            .or_insert_with(|| super::mesh_target::pipeline(&self.device, &self.scene, key).0);
        pass.set_pipeline(composite);
        pass.set_bind_group(2, &target.group, &[]);
        pass.draw(0..3, 0..1);
    }
    /// The selection a body draws with: the frame's specular options and winding, and the
    /// body's lighting and anchoring.
    fn selection(
        (options, front_face): ([u32; 7], wgpu::FrontFace),
        lighting: worldrunner_render_plan::SceneCelestialLighting,
        anchored: bool,
    ) -> [u32; 12] {
        let values = lighting.contributions_micro();
        [
            options[0],
            options[1],
            options[2],
            options[3],
            options[4],
            options[5],
            options[6],
            values[0],
            values[1],
            values[2],
            u32::from(anchored),
            u32::from(front_face == wgpu::FrontFace::Cw),
        ]
    }
    /// The celestial context of `raster` for a body with `lighting` and anchoring, made on
    /// first use on the scene's object family.
    fn context(
        &self,
        raster: ([u32; 7], wgpu::FrontFace),
        lighting: worldrunner_render_plan::SceneCelestialLighting,
        anchored: bool,
    ) -> crate::object::program::Context {
        let front_face = raster.1;
        let key = Self::selection(raster, lighting, anchored);
        self.contexts
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let composition = (self.physical, self.separate, self.object_exposure);
                crate::object::program::Context::new(
                    crate::object::program::Description {
                        device: self.device.clone(),
                        source: crate::renderer::celestial_source(composition, lighting, anchored)
                            .into(),
                        colors: vec![wgpu::TextureFormat::Rgba16Float],
                        samples: self.samples,
                        front_face,
                        depth: crate::renderer::depth::DepthMode::Independent,
                        previous_transforms: false,
                        constants: crate::renderer::celestial_constants(raster, lighting),
                        frame: self.scene.frame_layout().clone(),
                        shadow: None,
                        screen: None,
                        entry: crate::object::program::Entry::Celestial,
                    },
                    self.scene.object_context().family(),
                )
            })
            .clone()
    }
    /// The specular options and winding the bodies of a frame `state` draw with.
    fn raster(state: &crate::frame::FrameUniform) -> ([u32; 7], wgpu::FrontFace) {
        (
            state.specular_convention.shader_options(),
            state.front_face(),
        )
    }
    /// Prepares the pipeline of every body for the frame `state` before the pass that draws
    /// them opens, where a refusal is still an error rather than a lost body.
    ///
    /// # Errors
    /// Fails when the runtime refuses a body's program, material set or pipeline.
    pub(crate) fn prepare(
        &self,
        state: &crate::frame::FrameUniform,
        objects: &crate::object::prepared::PreparedObjects,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        let raster = Self::raster(state);
        for draw in &self.draws {
            let context = self.context(raster, draw.lighting, draw.anchored);
            objects
                .pipeline(
                    &context,
                    (draw.key.index(), draw.material_index),
                    crate::object::layout::GeometryLayout::Static,
                )
                .map_err(
                    |error| worldrunner_render_post::FrameError::FeaturePreparation {
                        feature: "celestial meshes",
                        reason: error.to_string(),
                    },
                )?;
        }
        Ok(())
    }
    /// Records the bodies through the program runtime: each binds the prepared pipeline of its
    /// key and material, the frame group at set 0, and draws its one instance.
    pub fn draw_geometry(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &wgpu::BindGroup,
        state: &crate::frame::FrameUniform,
        objects: &crate::object::prepared::PreparedObjects,
    ) {
        use worldrunner_render_program::runtime::{
            GeometryDraw, ImportedGeometry, ImportedIndices,
        };
        let raster = Self::raster(state);
        #[cfg(test)]
        if self.recorder.get() == Recorder::Nothing {
            return;
        }
        let layout = crate::object::layout::vertex_layout(
            false,
            crate::object::layout::GeometryLayout::Static,
        );
        for draw in &self.draws {
            // A selection that `prepare` did not see would be made here, where a refusal can only
            // panic: every frame that records the bodies must have been prepared with its raster.
            debug_assert!(
                self.contexts.borrow().contains_key(&Self::selection(
                    raster,
                    draw.lighting,
                    draw.anchored
                )),
                "the celestial bodies were not prepared for the frame they are recorded in"
            );
            let context = self.context(raster, draw.lighting, draw.anchored);
            let pipeline = objects
                .pipeline(
                    &context,
                    (draw.key.index(), draw.material_index),
                    crate::object::layout::GeometryLayout::Static,
                )
                .expect("the celestial pass prepares its bodies before it opens");
            pipeline.bind(pass);
            pass.set_bind_group(0, frame, &[]);
            let geometry = GeometryDraw::new(
                layout,
                &[draw.vertices.clone(), draw.instance.clone()],
                &ImportedGeometry::Indexed(ImportedIndices {
                    buffer: draw.indices.clone(),
                    format: wgpu::IndexFormat::Uint32,
                    first: draw.first_index,
                    count: draw.count,
                    base_vertex: draw.base_vertex,
                }),
                0..1,
            )
            .expect("a body's buffers hold its one level and its pose");
            pipeline
                .draw(pass, &geometry)
                .expect("the geometry was made for the objects' vertex layout");
        }
    }
}
#[cfg(test)]
impl Meshes {
    /// The bodies `draw_geometry` records.
    pub(crate) fn draw_count(&self) -> usize {
        self.draws.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneDirection};
    fn settings() -> SceneCelestialMesh {
        SceneCelestialMesh {
            lighting: worldrunner_render_plan::SceneCelestialLighting::NEUTRAL,
            tint: worldrunner_render_plan::SceneTint::WHITE,
            anchor_millimeters: None,
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
        let _session = crate::test_support::SceneTestSession::acquire();
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
    fn celestial_programs_validate_against_the_resident_material_layout() {
        let _session = crate::test_support::SceneTestSession::acquire();
        let context = crate::device::GpuContext::shared_for_test().unwrap();
        let scope = context
            .device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let scene = crate::renderer::ScenePipelines::new(
            context.device(),
            wgpu::TextureFormat::Rgba16Float,
            1,
        )
        .unwrap();
        let family = scene.object_context().family();
        let authority = context.submission();
        for physical in [false, true] {
            let meshes = Meshes::new(context.device(), &scene, Vec::new(), physical, false, false);
            assert_eq!(meshes.physical, physical);
            for roughness in 0..3 {
                for visibility in 0..2 {
                    for occlusion in 0..2 {
                        let celestial = meshes.context(
                            (
                                [roughness, visibility, occlusion, 0, 0, 0, 0],
                                wgpu::FrontFace::Ccw,
                            ),
                            worldrunner_render_plan::SceneCelestialLighting::NEUTRAL,
                            false,
                        );
                        family
                            .with_objects((context.device(), authority), |runtime| {
                                for key in crate::pipeline::PipelineKey::ALL {
                                    celestial.program(
                                        runtime,
                                        key,
                                        crate::object::layout::GeometryLayout::Static,
                                    )?;
                                }
                                Ok::<_, worldrunner_render_program::error::ProgramError>(())
                            })
                            .expect("every key's celestial program builds");
                    }
                }
            }
        }
        assert!(pollster::block_on(scope.pop()).is_none());
    }
}
