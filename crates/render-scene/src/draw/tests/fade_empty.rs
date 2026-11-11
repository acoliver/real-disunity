use super::*;
use worldrunner_render_plan::{
    SceneFadeBand, SceneFadeDirection, SceneFadeQuantization, SceneFadeReference,
};

mod readback;

#[derive(Clone, Copy, Debug)]
struct Case {
    band: [u32; 2],
    selected: bool,
    visible: bool,
}

fn boundary_cases(direction: SceneFadeDirection) -> impl Iterator<Item = Case> {
    // At 4 m: each pair records CPU membership and actual byte-quantized visibility.
    [
        ([1000, 2000], [false, false], [true, true]),
        ([1999, 3999], [false, false], [true, true]),
        ([2000, 4000], [false, false], [true, true]),
        ([2001, 4001], [false, false], [true, true]),
        ([4000, 6000], [true, true], [true, false]),
        ([6000, 8000], [true, true], [false, false]),
        ([2000, 6000], [true, true], [true, true]),
    ]
    .into_iter()
    .map(move |(band, keep, complement)| {
        let [selected, visible] = match direction {
            SceneFadeDirection::Keep => keep,
            SceneFadeDirection::Complement => complement,
            SceneFadeDirection::Inclusive => unreachable!(),
        };
        Case {
            band,
            selected,
            visible,
        }
    })
}

fn boundary_scene(impostor: bool, direction: SceneFadeDirection, band: [u32; 2]) -> RenderScene {
    let base = super::fade_handoff::handoff_scene(true, impostor, &[direction]);
    let material = base.resources().materials()[0]
        .with_fade_reference(SceneFadeReference::new([0, 0, 1000]))
        .with_fade_quantization(SceneFadeQuantization::Byte);
    let band = SceneFadeBand::new(band[0], band[1]).unwrap();
    let band = if direction == SceneFadeDirection::Keep {
        band.with_terminal_fade()
    } else {
        band
    };
    let object = base.geometry().objects()[0].with_fade(band);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), vec![material])
            .unwrap(),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), vec![object]).unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

fn assert_membership(source: &RenderScene, case: Case, casters_only: bool) {
    use crate::draw::visibility::{MeshSelection, Selection};
    let mut instances = crate::instances::Instances::new(source);
    let key = *instances.batches.keys().next().unwrap();
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let frame = frame_from(CameraPose {
        eye_millimeters: [0, -4000, 1000],
        target_millimeters: [0, 0, 1000],
    });
    for disabled in [true, false] {
        instances.fade_cull_disabled = disabled;
        let expected = usize::from(disabled || casters_only || case.selected);
        let mut selected = Selection {
            casters_only,
            ..Selection::default()
        };
        selected.rebuild_with_eye(
            &instances,
            &meshes,
            frame.view_projection,
            frame.camera_position_millimeters,
            frame.camera_position,
        );
        assert_eq!(selected.stats.considered, 1);
        assert_eq!(selected.stats.culled, 1 - expected, "{case:?}");
        assert_eq!(selected.stats.drawn_per_lod, [expected]);
        assert_eq!(selected.ranges.len(), expected, "{case:?}");
        for range in &selected.ranges {
            assert_eq!(range.key, key);
            assert_eq!((range.key.1, range.key.2), (0, 0));
            assert_eq!(range.lod, 0);
            assert_eq!(range.slots, 0..1);
        }
    }
}

#[test]
fn boundary_fixture_selects_each_isolated_case_and_preserves_survivors() {
    for impostor in [false, true] {
        for direction in [SceneFadeDirection::Keep, SceneFadeDirection::Complement] {
            for case in boundary_cases(direction) {
                let source = boundary_scene(impostor, direction, case.band);
                for casters_only in [false, true] {
                    assert_membership(&source, case, casters_only);
                }
            }
        }
    }
}

fn written_depth(attachments: &[Vec<u8>]) -> usize {
    attachments
        .last()
        .unwrap()
        .chunks_exact(4)
        .filter(|v| *v != 1.0_f32.to_le_bytes())
        .count()
}

fn compare_cases(
    context: &GpuContext,
    impostor: bool,
    direction: SceneFadeDirection,
    kind: readback::Kind,
) {
    use readback::Cull;
    let full_band = match direction {
        SceneFadeDirection::Keep => [6000, 8000],
        SceneFadeDirection::Complement => [1000, 2000],
        SceneFadeDirection::Inclusive => unreachable!(),
    };
    let full_scene = boundary_scene(impostor, direction, full_band);
    let (full, _) = readback::capture(context, &full_scene, kind, Cull::Disabled);
    // Dropping the isolated half-covered object yields the same clear bytes as any wrong cull.
    let partial_scene = boundary_scene(impostor, direction, [2000, 6000]);
    let (empty, dropped) = readback::capture(context, &partial_scene, kind, Cull::ForceEmpty);
    assert_eq!(dropped.calls, 0);
    assert_eq!(dropped.instances, 0);
    assert_eq!(written_depth(&empty), 0);
    assert!(written_depth(&full) > 0);
    let mut total_before = super::super::DrawCount::default();
    let mut total_after = super::super::DrawCount::default();
    for case in boundary_cases(direction) {
        let source = boundary_scene(impostor, direction, case.band);
        let (unculled, before) = readback::capture(context, &source, kind, Cull::Disabled);
        let (culled, after) = readback::capture(context, &source, kind, Cull::Enabled);
        assert_eq!(before.calls, 1);
        assert_eq!(before.instances, 1);
        // Shadow casters are never fade culled.
        let drawn = u64::from(case.selected || matches!(kind, readback::Kind::Shadow));
        assert_eq!(after.calls, drawn, "{case:?}");
        assert_eq!(after.instances, drawn, "{case:?}");
        assert_eq!(
            culled, unculled,
            "{kind:?}, impostor={impostor}, direction={direction:?}, {case:?}"
        );
        // A wrong removal of any visible boundary or partial case must change the raw bytes.
        assert_eq!(unculled != empty, case.visible, "{kind:?}, {case:?}");
        assert_eq!(written_depth(&unculled) > 0, case.visible, "{case:?}");
        if case.band == [2000, 6000] {
            assert!(written_depth(&unculled) < written_depth(&full));
        }
        total_before.calls += before.calls;
        total_before.instances += before.instances;
        total_after.calls += after.calls;
        total_after.instances += after.instances;
    }
    if matches!(kind, readback::Kind::Shadow) {
        assert_eq!(total_after.calls, total_before.calls);
        assert_eq!(total_after.instances, total_before.instances);
    } else {
        assert!(total_after.calls < total_before.calls);
        assert!(total_after.instances < total_before.instances);
    }
}

#[test]
fn gpu_fade_empty_cull_preserves_colour_depth_mrts_velocity_and_metadata() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for impostor in [false, true] {
        for direction in [SceneFadeDirection::Keep, SceneFadeDirection::Complement] {
            for kind in [
                readback::Kind::Forward,
                readback::Kind::Velocity,
                readback::Kind::Reflections,
                readback::Kind::Metadata,
            ] {
                compare_cases(&context, impostor, direction, kind);
            }
        }
    }
}

#[test]
fn gpu_fade_empty_cull_keeps_every_shadow_caster() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for impostor in [false, true] {
        for direction in [SceneFadeDirection::Keep, SceneFadeDirection::Complement] {
            compare_cases(&context, impostor, direction, readback::Kind::Shadow);
        }
    }
}
