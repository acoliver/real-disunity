//! Unproduced texels must not influence any published frame product.
#[path = "receiver_payload_detection.rs"]
mod detection;
use super::*;
use crate::scene_metadata::receiver::poison::ConsumerFamily;
#[path = "receiver_payload_surfaces.rs"]
mod surfaces;

fn modes() -> [(Option<ReceiverReference>, ReceiverReference); 3] {
    [
        (None, ReceiverReference::PoisonUncovered),
        (
            Some(ReceiverReference::StoredShared),
            ReceiverReference::PoisonShared,
        ),
        (
            Some(ReceiverReference::StoredIndependent),
            ReceiverReference::PoisonIndependent,
        ),
    ]
}

fn opaque_receivers(source: &RenderScene) -> RenderScene {
    let resources = SceneResources::new(
        source.resources().textures().to_vec(),
        source
            .resources()
            .materials()
            .iter()
            .copied()
            .map(|material| material.with_translucency(0).unwrap())
            .collect(),
    )
    .unwrap();
    source
        .with_resources(
            resources,
            source.geometry().clone(),
            source.effects().cloned(),
        )
        .unwrap()
}

fn water_receivers() -> RenderScene {
    let source = surfaces::fixture();
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_water_bodies(vec![worldrunner_render_plan::SceneWaterBody {
            material: worldrunner_render_plan::SceneWaterSettings::STOCK,
            surface: worldrunner_render_plan::SceneWaterSurface::Rectangle,
            flow_millimeters_per_second: [0; 2],
            height_millimeters: 500,
            east_min_millimeters: OFFSET[0] - 3_000,
            north_min_millimeters: OFFSET[1] - 3_000,
            east_extent_millimeters: 3_000,
            north_extent_millimeters: 6_000,
            color_micro: [50_000, 200_000, 250_000],
            absorption_micro: [300_000, 100_000, 50_000],
            roughness_micro: 200_000,
        }])
        .unwrap();
    source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

fn coverage_frame(
    context: &GpuContext,
    source: &RenderScene,
    reference: Option<ReceiverReference>,
    bias: f32,
    quad: bool,
) -> Vec<Vec<u8>> {
    let device = context.device();
    let mut frame = ReceiverFrame::new(context, source, reference, bias);
    if quad {
        let (gpu, pipelines) = frame.shadowed.scene_and_pipelines_mut();
        gpu.set_dynamic_contribution(
            device,
            context.submission(),
            pipelines,
            &[],
            &[crate::dynamic::DynamicQuad {
                position: crate::geometry::millimeters_to_meters([
                    OFFSET[0],
                    OFFSET[1] - 6_000,
                    2_000,
                ])
                .to_array(),
                tint: [0.3, 0.8, 0.2, 0.5],
                ..Default::default()
            }],
        )
        .unwrap();
    }
    let products = frame.capture(context, source, reference, receiver_pose());
    let payload = frame
        .renderer
        .ssao
        .as_ref()
        .unwrap()
        .receiver
        .as_ref()
        .unwrap();
    assert_eq!(
        payload.layout,
        if bias == 0.0 {
            ReceiverLayout::Front
        } else {
            ReceiverLayout::Opposing
        }
    );
    products
}

fn assert_uncovered(name: &str, cleared: &[Vec<u8>], poison: &[Vec<u8>]) {
    assert_eq!(cleared.len(), poison.len());
    let payload = cleared.len() - 2;
    assert!(
        cleared[payload..] != poison[payload..],
        "control must change uncovered payload texels"
    );
    assert!(
        cleared[1]
            .chunks_exact(4)
            .any(|p| p == 1.0_f32.to_le_bytes()),
        "fixture must contain background pixels"
    );
    let differences: Vec<_> = cleared
        .iter()
        .zip(poison)
        .map(|(a, b)| {
            assert_eq!(a.len(), b.len());
            a.iter().zip(b).filter(|(a, b)| a != b).count()
        })
        .collect();
    println!("POISON_CONTROL {name} attachment_bytes={differences:?}");
    for (attachment, differences) in differences[..payload].iter().enumerate() {
        assert_eq!(
            *differences, 0,
            "{name}: uncovered payload affected attachment={attachment}"
        );
    }
}

#[test]
fn gpu_receiver_consumers_do_not_publish_uncovered_payload_values() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for (name, source) in [
        ("transmission", surfaces::fixture()),
        ("opaque", opaque_receivers(&surfaces::fixture())),
    ] {
        for bias in [0.0, 0.012] {
            for (baseline, poisoned) in modes() {
                let cleared = frame(&context, &source, baseline, bias);
                let poison = frame(&context, &source, Some(poisoned), bias);
                assert_uncovered(
                    &format!("{name} bias={bias} {baseline:?}"),
                    &cleared,
                    &poison,
                );
                detection::assert_poison_corner(
                    &cleared,
                    &poison,
                    if name == "transmission" && bias != 0.0 {
                        2
                    } else {
                        1
                    },
                );
            }
        }
    }
}

#[test]
fn gpu_receiver_payload_clear_preserves_forward_only_winners() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = water_receivers();
    for bias in [0.0, 0.012] {
        for (baseline, poisoned) in modes() {
            let cleared = coverage_frame(&context, &source, baseline, bias, true);
            let poison = coverage_frame(&context, &source, Some(poisoned), bias, true);
            assert_uncovered(
                &format!("water + unpaired quad {baseline:?} bias={bias}"),
                &cleared,
                &poison,
            );
            detection::assert_poison_corner(&cleared, &poison, if bias == 0.0 { 1 } else { 2 });
            let no_quad = coverage_frame(&context, &source, baseline, bias, false);
            assert_ne!(
                cleared[0], no_quad[0],
                "unpaired draw must affect scene colour"
            );
            let payload = cleared.len() - 2;
            assert_eq!(
                cleared[payload..],
                no_quad[payload..],
                "unpaired draw must not produce payload"
            );
        }
    }
}
