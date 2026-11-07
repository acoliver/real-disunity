//! Scene-authored resolves use the retained frame executor and the existing edge fixture.
use super::taa_end_to_end_tests::{OUTPUT, SIZE, Scene, Target, context, frame, texture};
use crate::{compositor::FrameRenderer, device::GpuContext, post_taa_tests::effects};
use worldrunner_render_plan::{SceneAntiAlias, SceneSmaaPreset};
use worldrunner_render_post::{
    AntiAliasing, FrameConfig, FrameError, MsaaSamples, PassId, PostResolve, Readback, StageId,
    TimingMode, ToneMap,
};

struct Rig<'a> {
    context: &'a GpuContext,
    renderer: FrameRenderer,
    scene: Scene,
    output: wgpu::Texture,
    readback: Readback,
    submitted: usize,
}

impl<'a> Rig<'a> {
    fn new(context: &'a GpuContext, authored: SceneAntiAlias, descriptor: PostResolve) -> Self {
        let renderer = FrameRenderer::hdr_antialiased(
            context,
            [SIZE, SIZE],
            OUTPUT,
            AntiAliasing::new(MsaaSamples::X1, descriptor),
            ToneMap::Reinhard,
            Some(&effects(authored, None)),
        )
        .unwrap();
        let device = context.device();
        Self {
            context,
            renderer,
            scene: Scene::new(device, 1),
            output: texture(device, SIZE, OUTPUT),
            readback: Readback::new(device, FrameConfig::new([SIZE, SIZE], OUTPUT, 1), 1 << 20)
                .unwrap(),
            submitted: 0,
        }
    }

    fn render(&mut self, eye_x: i64, timed: bool) -> Vec<u8> {
        let device = self.context.device();
        self.renderer.select_frame_slot(self.submitted % 3);
        self.submitted += 1;
        let prepared = self.renderer.prepare_uniform(frame(eye_x, SIZE)).unwrap();
        let view = self
            .output
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (renderer, scene) = (&mut self.renderer, &self.scene);
        let frame_id = u64::try_from(self.submitted).unwrap();
        self.context
            .submission()
            .prepare(device, |encoder, _| {
                if timed {
                    renderer.begin_timing(frame_id).unwrap();
                }
                let targets = renderer.targets();
                scene.draw(
                    device,
                    encoder,
                    &Target {
                        color: targets.scene(),
                        resolve: None,
                        depth: targets.depth(),
                    },
                    prepared.view_projection,
                );
                renderer.encode_frame(device, encoder, (&[], &[]), &mut [], (prepared, None))?;
                renderer.encode_output(encoder, &view).unwrap();
                Ok::<_, FrameError>(())
            })
            .unwrap()
            .submit()
            .unwrap();
        self.readback
            .read(device, self.context.submission(), &self.output)
            .unwrap()
    }
}

#[test]
fn scene_smaa_ultra_schedules_three_timed_stages() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = pollster::block_on(GpuContext::request_with_features(
        wgpu::Features::TIMESTAMP_QUERY,
    ))
    .unwrap();
    let mut rig = Rig::new(
        &context,
        SceneAntiAlias::smaa(SceneSmaaPreset::Ultra),
        PostResolve::None,
    );
    assert_eq!(rig.renderer.projection_jitter(), None);
    rig.renderer
        .configure_timing(context.device(), context.queue(), TimingMode::Required)
        .unwrap();
    rig.render(0, true);
    let timings = rig.renderer.take_timings().unwrap();
    assert_eq!(timings.len(), 1);
    assert_eq!(timings[0].frame_id, 1);
    let smaa: Vec<_> = timings[0]
        .passes
        .iter()
        .filter(|pass| matches!(pass.pass_id, PassId::Stage(id) if id.name.starts_with("smaa_")))
        .collect();
    assert_eq!(
        smaa.iter().map(|pass| pass.pass_id).collect::<Vec<_>>(),
        [
            PassId::Stage(StageId::new("smaa_edges", 0)),
            PassId::Stage(StageId::new("smaa_weights", 0)),
            PassId::Stage(StageId::new("smaa_blend", 0)),
        ]
    );
    assert!(!timings[0].passes.iter().any(|pass| matches!(pass.pass_id, PassId::Stage(id) if id.name == "fxaa" || id.name == "taa resolve")));
    assert!(!rig.renderer.has_temporal_history());
}

#[test]
fn scene_smaa_ultra_differs_from_fxaa_on_high_contrast_edges() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut smaa = Rig::new(
        &context,
        SceneAntiAlias::smaa(SceneSmaaPreset::Ultra),
        PostResolve::None,
    );
    let mut fxaa = Rig::new(
        &context,
        SceneAntiAlias::Fxaa {
            subpixel_micro: 1_000_000,
        },
        PostResolve::None,
    );
    let mut plain = Rig::new(&context, SceneAntiAlias::Disabled, PostResolve::None);
    let (smaa_image, fxaa_image, plain_image) = (
        smaa.render(0, false),
        fxaa.render(0, false),
        plain.render(0, false),
    );
    assert_ne!(smaa_image, fxaa_image);
    assert_ne!(smaa_image, plain_image, "SMAA must blend fixture edges");
    assert_ne!(fxaa_image, plain_image, "FXAA must blend fixture edges");
    assert_eq!(
        smaa_image,
        smaa.render(0, false),
        "SMAA has no jitter or history"
    );
}

#[test]
fn scene_fxaa_and_taa_match_the_existing_descriptor_path_bytes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for (authored, descriptor) in [
        (SceneAntiAlias::Disabled, PostResolve::None),
        (
            SceneAntiAlias::Fxaa {
                subpixel_micro: 750_000,
            },
            PostResolve::fxaa(0.75),
        ),
        (SceneAntiAlias::taa(900_000), PostResolve::taa(0.9)),
    ] {
        let mut actual = Rig::new(&context, authored, PostResolve::None);
        let mut control = Rig::new(&context, SceneAntiAlias::Disabled, descriptor);
        assert_eq!(
            actual.renderer.projection_jitter(),
            control.renderer.projection_jitter()
        );
        for eye_x in [0, 0, 0, 0, 41, 82, 123, 164, -29, -58, -87, -116] {
            assert_eq!(
                actual.render(eye_x, false),
                control.render(eye_x, false),
                "{authored:?} at {eye_x}"
            );
        }
    }
}
