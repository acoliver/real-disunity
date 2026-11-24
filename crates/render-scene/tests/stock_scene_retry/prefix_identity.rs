#[test]
fn public_stock_prefix_rejects_substituted_backing_before_caller_preparation() {
    assert_refusal(Change::Backing);
}

#[test]
fn public_stock_prefix_rejects_changed_load_before_caller_preparation() {
    assert_refusal(Change::Load);
}

use super::{Products, Refusal, Scene, fixture};
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, BorrowedFeature, FrameError, FramePlan,
    PassDeclaration, PassSlot, ResourceRef, color_texture,
};
use worldrunner_render_scene::{
    compositor::{PrefixContribution, PrefixScene, prepare_stock_scene},
    device::GpuContext,
    test_support::SceneTestSession,
    wgpu,
};

#[derive(Clone, Copy)]
enum Change {
    Backing,
    Load,
}

struct Substitution {
    pass: PassDeclaration,
    original: AttachmentView,
    replacement: AttachmentView,
}

impl Substitution {
    fn new(context: &GpuContext, scene: &Scene) -> Self {
        let size = [scene.output.width(), scene.output.height()];
        let format = wgpu::TextureFormat::Rgba16Float;
        let resource = ResourceRef::new("caller prefix attachment");
        let original = color_texture(context.device(), "completed prefix backing", size, format);
        let replacement =
            color_texture(context.device(), "substituted prefix backing", size, format);
        let mut pass = PassDeclaration::nth(PassSlot::DepthPyramid(0), 1);
        pass.color = Some(AttachmentDeclaration {
            resource,
            format,
            size,
            samples: 1,
            load: AttachmentLoad::ClearColor(wgpu::Color::RED),
            store: wgpu::StoreOp::Store,
            read_only: false,
        });
        assert_ne!(original, replacement);
        Self {
            pass,
            original: AttachmentView::new(resource, &original).unwrap(),
            replacement: AttachmentView::new(resource, &replacement).unwrap(),
        }
    }
}

fn encode_suffix(
    context: &GpuContext,
    scene: &mut Scene,
    encoder: &mut wgpu::CommandEncoder,
    substitution: &Substitution,
) -> Result<(), FrameError> {
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        scene.frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        scene.frame.targets().depth().texture(),
    )
    .unwrap();
    let mut stock = prepare_stock_scene(
        &mut scene.frame,
        context.device(),
        encoder,
        (&mut scene.gpu, &scene.pipelines),
        worldrunner_render_scene::compositor::ColorDepth {
            color: &color,
            depth: &depth,
        },
        scene.uniform,
    )?;
    let plan = stock.plan();
    let (mut passes, mut views) = (plan.passes.to_vec(), plan.views.to_vec());
    passes.push(substitution.pass.clone());
    views.push(substitution.replacement.clone());
    let (declaration, frame) = (stock.declaration(), stock.frame());
    let offscreen = stock.take_offscreen_operations();
    scene.frame.encode_frame(
        context.device(),
        encoder,
        (&passes, &views),
        offscreen.as_ref().map(|work| work as _),
        &mut [BorrowedFeature {
            declaration,
            prepared: &mut stock,
        }],
        &(frame, None),
    )
}

fn refuse_substitution(
    context: &GpuContext,
    scene: &mut Scene,
    change: Change,
) -> (FrameError, FrameError) {
    let mut substitution = Substitution::new(context, scene);
    let prefix = scene
        .frame
        .stock_depth_prefix(
            context.device(),
            PrefixScene {
                gpu: &mut scene.gpu,
                pipelines: &scene.pipelines,
            },
        )
        .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    prefix
        .encode(
            &mut scene.frame,
            context.device(),
            &mut encoder,
            PrefixScene {
                gpu: &mut scene.gpu,
                pipelines: &scene.pipelines,
            },
            scene.uniform,
            PrefixContribution {
                plan: &FramePlan {
                    passes: std::slice::from_ref(&substitution.pass),
                    views: std::slice::from_ref(&substitution.original),
                    requested: &[],
                    operations: &[],
                },
                features: &mut [],
            },
        )
        .unwrap();
    let expected = match change {
        Change::Backing => FrameError::AttachmentView(substitution.original.resource()),
        Change::Load => {
            substitution.replacement = substitution.original.clone();
            substitution.pass.color.as_mut().unwrap().load =
                AttachmentLoad::ClearColor(wgpu::Color::BLUE);
            FrameError::AttachmentContract(substitution.pass.entry())
        }
    };
    let error = encode_suffix(context, scene, &mut encoder, &substitution)
        .expect_err("a completed prefix attachment must keep its declaration and backing");
    (expected, error)
}

fn assert_refusal(change: Change) {
    let _session = SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    let validation = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let mut scene = Scene::new(&context, &fixture::source());
    let refusal = Refusal::default();
    scene
        .frame
        .register(context.device(), Box::new(refusal.clone()))
        .unwrap();
    scene.complete(&context);
    scene.seed_confidence(&context);
    let completed = Products::read(&context, &scene);
    completed.assert_confidence(0.5);
    let statistics = scene.gpu.statistics();
    let selection = scene.gpu.view_stats();
    let jitter = scene.frame.projection_jitter_index();
    let history = scene.frame.has_temporal_history();
    refusal.set_refusing(true);
    let submitted = context.submission().submitted_serial();
    let (expected, error) = refuse_substitution(&context, &mut scene, change);
    assert_eq!(error, expected);
    assert_eq!(context.submission().submitted_serial(), submitted);
    assert_eq!(refusal.preparations(), 1);
    assert_eq!(scene.gpu.statistics(), statistics);
    assert_eq!(scene.gpu.view_stats(), selection);
    assert_eq!(scene.frame.projection_jitter_index(), jitter);
    assert_eq!(scene.frame.has_temporal_history(), history);
    assert_eq!(Products::read(&context, &scene), completed);
    assert!(pollster::block_on(validation.pop()).is_none());
}
