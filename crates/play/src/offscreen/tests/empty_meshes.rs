//! A mesh with nothing to draw is skipped without a failure and counted by what it holds, while
//! a mesh that is wrong is a failure with a name. The meshes are synthetic.

use worldrunner_disunity_runtime::{
    MeshChannel, MeshChannels, MeshRange, PlayerTexture, TextureSampler,
};
use worldrunner_world_simulation::graphics::GraphicsSnapshot;

use super::super::assets::{AssetError, Assets};
use super::super::frame::{Adapted, RendererSkip};
use super::gpu::{MESH, Rig, SIDE, camera_render};
use super::support::{render_texture, world};

/// The only mesh the host stores, as given.
struct Stored(MeshChannels);

impl Assets for Stored {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Ok((identity == MESH && known != self.0.generation).then(|| self.0.clone()))
    }

    fn texture(&self, _: u64, _: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok(None)
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

fn triangle() -> MeshChannels {
    MeshChannels {
        generation: 1,
        vertex_count: 3,
        channels: vec![
            MeshChannel {
                number: 0,
                dimension: 3,
                data: vec![-1.0, -1.0, 0.5, 1.0, -1.0, 0.5, -1.0, 1.0, 0.5],
            },
            MeshChannel {
                number: 4,
                dimension: 2,
                data: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            },
        ],
        indices: vec![0, 1, 2],
        ranges: vec![MeshRange {
            start: 0,
            count: 3,
            base_vertex: 0,
            topology: 0,
        }],
    }
}

fn adapt(stored: MeshChannels) -> Adapted {
    let mut rig = Rig::new();
    let mut world = world();
    let target = render_texture(&mut world, [SIDE, SIDE]);
    camera_render(&mut world, target);
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    rig.adapter
        .adapt(&snapshot, &Stored(stored), &mut rig.state, &mut rig.runtime)
}

fn mesh_failures(adapted: &Adapted) -> Vec<(&str, usize)> {
    adapted
        .report
        .failures
        .iter()
        .filter(|(name, _)| name.starts_with("mesh"))
        .map(|(name, count)| (name.as_str(), *count))
        .collect()
}

#[test]
fn an_empty_mesh_is_a_census_line_and_not_a_failure() {
    let mut stored = triangle();
    stored.indices.clear();
    stored.ranges.clear();
    let adapted = adapt(stored);

    assert_eq!(mesh_failures(&adapted), []);
    assert_eq!(
        adapted.report.census["mesh with nothing to draw: 0 sub-meshes with topologies [], no indices, with vertices"],
        1
    );
    // The renderer that names it has no mesh to draw, as before.
    assert_eq!(
        adapted.report.renderers_skipped[&RendererSkip::MeshUnavailable],
        1
    );
}

#[test]
fn sub_meshes_of_no_indices_are_counted_with_their_topologies() {
    let mut stored = triangle();
    stored.ranges[0].count = 0;
    stored.ranges.push(MeshRange {
        start: 0,
        count: 0,
        base_vertex: 0,
        topology: 4,
    });
    let adapted = adapt(stored);

    assert_eq!(mesh_failures(&adapted), []);
    assert_eq!(
        adapted.report.census["mesh with nothing to draw: 2 sub-meshes with topologies [0, 4], with indices, with vertices"],
        1
    );
}

#[test]
fn a_sub_mesh_over_indices_the_mesh_lacks_is_a_named_failure() {
    let mut stored = triangle();
    stored.indices.clear();
    let adapted = adapt(stored);

    assert_eq!(mesh_failures(&adapted), [("mesh:IndicesMissing", 1)]);
    assert!(
        !adapted
            .report
            .census
            .keys()
            .any(|name| name.starts_with("mesh with nothing")),
    );
}
