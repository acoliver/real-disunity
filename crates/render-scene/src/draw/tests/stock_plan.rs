//! The stock frame plan of a scene frame: the passes it submits, how often it is rebuilt and
//! which allocations its attachments are bound to.
mod cache;
mod lazy_snapshots;
mod pins;
mod post_water_depth;
mod rebind;
mod water_draws;

use super::*;
use crate::compositor::plan_trace::{self, SubmittedPlan, assert_pinned};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use crate::test_support::upload_scene;
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
    scene_with(request, false)
}

/// A water body below the fixture's quad, so the frame reads snapshots and the water receivers.
fn water_body() -> worldrunner_render_plan::SceneWaterBody {
    worldrunner_render_plan::SceneWaterBody {
        material: worldrunner_render_plan::SceneWaterSettings::STOCK,
        surface: worldrunner_render_plan::SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: -100,
        east_min_millimeters: -4000,
        north_min_millimeters: -4000,
        east_extent_millimeters: 8000,
        north_extent_millimeters: 8000,
        color_micro: [50_000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50_000],
        roughness_micro: 200_000,
    }
}

fn scene_with(request: Option<SceneSsr>, water: bool) -> RenderScene {
    let mut effects = SceneEffects::new(effect_parts()).unwrap();
    if let Some(request) = request {
        effects = effects.with_ssr(request).unwrap();
    }
    if water {
        effects = effects.with_water_bodies(vec![water_body()]).unwrap();
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
    let mut settings = RendererConfiguration::new(size, COLOR_FORMAT, ToneMap::Reinhard);
    settings.frame.format = TextureFormat::Rgba16Float;
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

/// A configured renderer with a scene resident, drawn through `FrameEncoding::encode_scene`.
struct Rig<'a> {
    context: &'a GpuContext,
    source: RenderScene,
    renderer: ConfiguredFrameRenderer,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    size: [u32; 2],
}

/// The colour and depth a frame is drawn into, which the caller supplies.
#[derive(Clone)]
struct Caller {
    color: wgpu::Texture,
    depth: wgpu::Texture,
}

impl Caller {
    /// Allocations of its own with the descriptors of `other`.
    fn like(device: &wgpu::Device, other: &Self) -> Self {
        let allocate = |texture: &wgpu::Texture| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("caller attachment"),
                size: texture.size(),
                mip_level_count: texture.mip_level_count(),
                sample_count: texture.sample_count(),
                dimension: texture.dimension(),
                format: texture.format(),
                usage: texture.usage(),
                view_formats: &[],
            })
        };
        Self {
            color: allocate(&other.color),
            depth: allocate(&other.depth),
        }
    }
}

impl<'a> Rig<'a> {
    fn new(context: &'a GpuContext, request: Option<SceneSsr>) -> Self {
        Self::with_source(context, source(request))
    }

    fn with_water(context: &'a GpuContext, request: Option<SceneSsr>) -> Self {
        Self::with_source(context, scene_with(request, true))
    }

    fn with_source(context: &'a GpuContext, source: RenderScene) -> Self {
        let renderer = ConfiguredFrameRenderer::new(
            context,
            configuration(&source, SIZE),
            AllocationBudget(64 * 1024 * 1024),
        )
        .unwrap();
        let pipelines =
            ScenePipelines::new(context.device(), TextureFormat::Rgba16Float, 1).unwrap();
        let gpu =
            upload_scene(context.device(), context.submission(), &source, &pipelines).unwrap();
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

    /// The colour and depth the renderer allocated for itself.
    fn own_caller(&mut self) -> Caller {
        let targets = self.renderer.renderer_mut().targets();
        Caller {
            color: targets.scene().texture().clone(),
            depth: targets.depth().texture().clone(),
        }
    }

    fn draw(&mut self, slot: usize, viewpoint: CameraPose) {
        let caller = self.own_caller();
        self.draw_into(slot, viewpoint, &caller);
    }

    fn draw_into(&mut self, slot: usize, viewpoint: CameraPose, caller: &Caller) {
        let device = self.context.device();
        let renderer = self.renderer.renderer_mut();
        renderer.select_frame_slot(slot);
        let color = AttachmentView::new(ResourceRef::new("scene color"), &caller.color).unwrap();
        let depth = AttachmentView::new(ResourceRef::new("scene depth"), &caller.depth).unwrap();
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
        renderer
            .encoding()
            .encode_scene(
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

    fn read_snapshot(&mut self, slot: usize) -> Vec<u8> {
        let texture = self.snapshot(slot);
        self.read(&texture)
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
