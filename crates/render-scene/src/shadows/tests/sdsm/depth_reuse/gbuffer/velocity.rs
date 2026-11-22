use super::*;
use crate::terrain::deferred_tests::halves;
use worldrunner_render_plan::{SceneAmbientOcclusion, SceneMaterialFeatures, SceneTint, SceneWind};

// The existing screen-space velocity tests bound half-float residuals to 0.064 px at 64 px.
const TOLERANCE_UV: f32 = 1e-3;

pub(super) fn temporal_source() -> RenderScene {
    let base = render_source();
    let mut parts = fixture::effects_parts(600_000);
    parts.anti_alias = SceneAntiAlias::taa(900_000);
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_opaque_path(SceneOpaquePath::GBUFFER_RENDER)
        .unwrap()
        .with_depth_fitting(*base.effects().unwrap().depth_fitting().unwrap())
        .unwrap();
    let wind = SceneWind::new(
        SceneDirection::new([1_000_000, 0, 0]).unwrap(),
        500,
        1000,
        0,
    )
    .unwrap();
    let materials = base
        .resources()
        .materials()
        .iter()
        .map(|material| {
            material
                .with_features(SceneMaterialFeatures {
                    tint: SceneTint::WHITE,
                    ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
                    wind: Some(wind),
                    layer: None,
                    translucency: None,
                })
                .unwrap()
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: base.resources().clone().with_materials(materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

pub(super) fn temporal_fixture(
    context: &GpuContext,
    source: &RenderScene,
) -> (FrameRenderer, ShadowedScene) {
    let viewport = view().viewport();
    let frame = FrameRenderer::hdr(
        context,
        [viewport.width, viewport.height],
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let mut shadows = settings();
    shadows.normal_bias_meters = 0.0;
    let renderer = ShadowedScene::with_taa_velocity(
        context,
        source,
        shadows,
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    (frame, renderer)
}

fn temporal_views(source: &RenderScene) -> [SceneView; 2] {
    let moved = SceneView::new(
        crate::CameraPose {
            eye_millimeters: [400, -4_000, 6_500],
            target_millimeters: [0, 0, 0],
        },
        *source.effects().unwrap().camera(),
        view().viewport(),
    )
    .unwrap();
    assert_ne!(
        view().view_projection_matrix(),
        moved.view_projection_matrix()
    );
    [view(), moved]
}

struct Residual {
    velocity: Vec<f32>,
    g3: Vec<u8>,
}

fn residual_after(
    context: &GpuContext,
    source: &RenderScene,
    fitting: Option<SdsmSettings>,
) -> Residual {
    let (mut frame, mut renderer) = temporal_fixture(context, source);
    renderer.set_depth_fitting(context, fitting).unwrap();
    let velocity = frame.targets().velocity_texture().unwrap().clone();
    let (color, depth) = attachments(&frame);
    for (id, (view, seconds)) in temporal_views(source)
        .into_iter()
        .zip([0.125, 0.25])
        .enumerate()
    {
        frame.set_frame_time(std::time::Duration::from_secs_f32(seconds));
        if fitting.is_some() {
            let id = u64::try_from(id).unwrap();
            let fit = renderer
                .render_depth_fitted_with(context, &view, (&color, &depth), id, &mut frame)
                .unwrap()
                .unwrap();
            assert_eq!((fit.source_frame, fit.adopted_frame), (id, id));
        } else {
            let mut encoder = context.submission().begin(context.device()).unwrap();
            renderer
                .encode_with(
                    context,
                    &view,
                    (&color, &depth),
                    &mut encoder,
                    Some(&mut frame),
                )
                .unwrap();
            encoder.finish().submit().unwrap();
        }
        assert_eq!(frame.targets().velocity_texture().unwrap(), &velocity);
    }
    Residual {
        velocity: halves(&read(context, &velocity)),
        g3: read(
            context,
            frame
                .feature_target(FeatureTarget::Gbuffer(GbufferTarget::G3))
                .unwrap(),
        ),
    }
}

fn assert_residual_matches(fitted: &Residual, baseline: &Residual) {
    assert_eq!(
        fitted.g3, baseline.g3,
        "both schedules must store the same surfaces"
    );
    assert_eq!(fitted.velocity.len(), baseline.velocity.len());
    assert_eq!(fitted.velocity.len(), fitted.g3.len());
    let mut moving = 0;
    let mut largest = 0.0_f32;
    for ((got, want), code) in fitted
        .velocity
        .chunks_exact(2)
        .zip(baseline.velocity.chunks_exact(2))
        .zip(fitted.g3.chunks_exact(2))
    {
        if u16::from_le_bytes([code[0], code[1]]) == 0 {
            assert_eq!(got, [0.0, 0.0], "uncovered velocity must stay cleared");
            continue;
        }
        assert!(got.iter().all(|value| value.is_finite()));
        assert!(
            (got[0] - want[0]).abs() < TOLERANCE_UV && (got[1] - want[1]).abs() < TOLERANCE_UV,
            "fitted residual {got:?}, non-SDSM residual {want:?}"
        );
        if want[0].abs() > 1e-6 || want[1].abs() > 1e-6 {
            moving += 1;
            largest = largest.max(got[0].abs()).max(got[1].abs());
        }
    }
    assert!(moving >= 200, "only {moving} covered texels carry motion");
    assert!(largest > 0.01, "the prefix residual was lost: {largest}");
}

#[test]
fn gpu_same_frame_gbuffer_preserves_taa_velocity_across_camera_poses() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = temporal_source();
    let baseline = residual_after(&context, &source, None);
    let fitted = residual_after(
        &context,
        &source,
        Some(SdsmSettings {
            policy: AdoptionPolicy::SameFrameCapture,
            range: policy(),
        }),
    );
    assert!(pollster::block_on(scope.pop()).is_none());
    assert_residual_matches(&fitted, &baseline);
}
