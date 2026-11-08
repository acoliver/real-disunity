//! Exercise the same live-batch predicate used by forward and SDSM eligibility.
use super::super::visibility::{MeshSelection, has_relief};
use super::*;
use crate::{instances::Instances, pipeline::PipelineKey, renderer::depth::DepthMode};

fn fixture(transition: bool) -> (Instances, Vec<MeshSelection>) {
    let source = if transition {
        lod_material::transition_scene(500_000)
    } else {
        lod_material::scene(true)
    };
    let relief_slot = if transition { 0 } else { 2 };
    let impostor = worldrunner_render_plan::SceneImpostor::new(
        worldrunner_render_plan::SceneImpostorLayout::HemisphericalOctahedral,
        [2, 2],
        0,
        false,
    )
    .unwrap()
    .with_relief(TextureHandle::from_raw(1), 100)
    .unwrap();
    let mut materials = source.resources().materials().to_vec();
    materials[relief_slot] = materials[relief_slot].with_impostor(impostor);
    let source = source
        .with_resources(
            SceneResources::new(source.resources().textures().to_vec(), materials).unwrap(),
            source.geometry().clone(),
            source.effects().cloned(),
        )
        .unwrap();
    let meshes = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let instances = Instances::new(&source);
    assert!(
        instances
            .batches
            .keys()
            .all(|key| !PipelineKey::ALL[key.0].relief)
    );
    (instances, meshes)
}

fn eligible(instances: &Instances, meshes: &[MeshSelection]) -> DepthMode {
    crate::compositor::depth_reuse::mode(1, true, has_relief(instances, meshes))
}

#[test]
fn relief_lod_slot_excludes_non_relief_default_from_depth_reuse() {
    let (instances, meshes) = fixture(false);
    assert_eq!(eligible(&instances, &meshes), DepthMode::Shared);
}

#[test]
fn relief_on_preceding_transition_half_excludes_depth_reuse() {
    let (instances, meshes) = fixture(true);
    assert!(!PipelineKey::ALL[instances.pipelines[2]].relief);
    let mut selected = super::super::visibility::Selection::default();
    let frame = frame_from(pose());
    selected.rebuild(
        &instances,
        &meshes,
        frame.view_projection,
        frame.camera_position_millimeters,
    );
    assert!(!selected.ranges.is_empty());
    assert!(selected.ranges.iter().all(|range| range.lod == 1));
    let source = lod_material::transition_scene(500_000);
    assert!(
        super::super::PreparedScene::new(&source).unwrap().meshes[0].lods[1]
            .transition
            .is_some()
    );
    assert_eq!(eligible(&instances, &meshes), DepthMode::Shared);
}

#[test]
fn removed_and_empty_batches_do_not_exclude_depth_reuse() {
    let (mut instances, meshes) = fixture(false);
    let handles: Vec<_> = instances.handles().collect();
    for handle in handles {
        instances.remove(handle).unwrap();
    }
    assert!(!instances.batches.is_empty());
    assert_eq!(eligible(&instances, &meshes), DepthMode::Reuse);
    let relief = PipelineKey::ALL.iter().position(|key| key.relief).unwrap();
    instances.batches.entry((relief, 0, 2)).or_default();
    assert_eq!(eligible(&instances, &meshes), DepthMode::Reuse);
}
