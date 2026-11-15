//! The stock frame plan of a scene frame: the passes it submits, how often it is rebuilt and
//! which allocations its attachments are bound to.
mod pins;

use super::*;
use crate::compositor::plan_trace::{self, SubmittedPlan, assert_pinned};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration, encode_scene};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::SceneSsr;
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef, ToneMap};

const SIZE: [u32; 2] = [65, 49];

fn reflections() -> SceneSsr {
    SceneSsr {
        enabled: true,
        max_steps: 64,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 500_000,
        ..Default::default()
    }
}

fn source(request: Option<SceneSsr>) -> RenderScene {
    let mut effects = SceneEffects::new(effect_parts()).unwrap();
    if let Some(request) = request {
        effects = effects.with_ssr(request).unwrap();
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture(0xffff)],
            vec![material(SceneAlphaMode::Opaque, true)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad(), card(None)],
            vec![
                SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                ),
                SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(1),
                    MaterialHandle::from_raw(0),
                ),
            ],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

fn configuration(source: &RenderScene, size: [u32; 2]) -> RendererConfiguration {
    let mut settings = RendererConfiguration::legacy(size, COLOR_FORMAT);
    settings.frame.format = TextureFormat::Rgba16Float;
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    let effects = source.effects().unwrap();
    settings.ssao = effects.ssao().copied();
    settings.ssr = effects.ssr().copied();
    settings.water = effects.water_bodies().to_vec();
    settings.post = crate::post::settings(effects).unwrap();
    settings
}

/// Four viewpoints that frame the fixture differently, so what a frame draws says which frame
/// it was.
fn viewpoint(index: usize) -> CameraPose {
    let eyes = [
        [0, -2_000, 2_000],
        [0, -1_000, 6_000],
        [2_500, -2_000, 1_500],
        [-2_500, 2_000, 2_500],
    ];
    CameraPose {
        eye_millimeters: eyes[index],
        target_millimeters: [0, 0, 0],
    }
}

/// A configured renderer with a scene resident, drawn through `encode_scene`.
struct Rig<'a> {
    context: &'a GpuContext,
    source: RenderScene,
    renderer: ConfiguredFrameRenderer,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    size: [u32; 2],
}

impl<'a> Rig<'a> {
    fn new(context: &'a GpuContext, request: Option<SceneSsr>) -> Self {
        let source = source(request);
        let renderer = ConfiguredFrameRenderer::new(
            context,
            configuration(&source, SIZE),
            AllocationBudget(64 * 1024 * 1024),
        )
        .unwrap();
        let pipelines =
            ScenePipelines::with_samples(context.device(), TextureFormat::Rgba16Float, 1).unwrap();
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
        Self {
            context,
            source,
            renderer,
            gpu,
            pipelines,
            size: SIZE,
        }
    }

    fn reconfigure(&mut self, size: [u32; 2]) {
        self.size = size;
        self.renderer
            .reconfigure(self.context, configuration(&self.source, size))
            .unwrap();
    }

    fn draw(&mut self, slot: usize, viewpoint: CameraPose) {
        let device = self.context.device();
        let renderer = self.renderer.renderer_mut();
        renderer.select_frame_slot(slot);
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let view = SceneView::new(
            viewpoint,
            camera(),
            Viewport {
                width: self.size[0],
                height: self.size[1],
            },
        )
        .unwrap();
        let mut encoder = self.context.submission().begin(device).unwrap();
        encode_scene(
            renderer,
            device,
            &mut encoder,
            (&mut self.gpu, &self.pipelines),
            (&color, &depth),
            FrameUniform::new(&view, self.source.effects()).unwrap(),
        )
        .unwrap();
        encoder.finish().submit().unwrap();
    }

    /// The colour snapshot bank of `slot`, which the renderer allocates when a frame first
    /// needs it.
    fn snapshot(&mut self, slot: usize) -> wgpu::Texture {
        self.renderer
            .renderer_mut()
            .snapshots
            .as_ref()
            .expect("the reflections read the opaque colour snapshot")[slot]
            .color
            .clone()
    }

    fn read(&self, texture: &wgpu::Texture) -> Vec<u8> {
        Readback::new(
            self.context.device(),
            FrameConfig::new(self.size, TextureFormat::Rgba16Float, 1),
            1_000_000,
        )
        .unwrap()
        .read(self.context.device(), self.context.submission(), texture)
        .unwrap()
    }
}

fn first_frame(request: Option<SceneSsr>) -> Vec<SubmittedPlan> {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = Rig::new(&context, request);
    plan_trace::take();
    rig.draw(0, viewpoint(0));
    plan_trace::take()
}

#[test]
fn gpu_forward_scene_frame_submits_its_pinned_plan() {
    assert_pinned("forward scene", &first_frame(None), pins::FORWARD);
}

#[test]
fn gpu_reflection_scene_frame_submits_its_pinned_plan() {
    assert_pinned(
        "reflection scene",
        &first_frame(Some(reflections())),
        pins::REFLECTIONS,
    );
}
