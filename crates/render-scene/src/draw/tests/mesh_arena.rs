//! Mesh levels share buffers: where they sit, what they cost in the memory census, and that
//! drawing them through shared buffers writes the pixels that drawing them from buffers of
//! their own writes.
//!
//! Uploading with a chunk limit of one byte gives every level a buffer of its own, which is how
//! levels were held before they shared buffers, so the same scene is drawn both ways in one
//! build.
use super::compaction::{Rig, cards, pose_at, unlit};
use super::skinning::{ribbon, shifted, skeleton};
use super::*;
use crate::draw::{PreparedMeshReplacement, with_chunk_limit};
use crate::object::layout::GeometryLayout;
use worldrunner_render_gpu::census::ResourceClass;
use worldrunner_render_plan::{SceneAffineMatrix, SceneCard, SceneSkinPose, SkinPoseHandle};

/// The ribbon without influences, as a rigid mesh.
fn rigid_ribbon() -> SceneMesh {
    let source = ribbon();
    let lod = &source.lods()[0];
    SceneMesh::new(vec![
        SceneMeshLod::new(0, lod.vertices().to_vec(), lod.indices().to_vec(), None).unwrap(),
    ])
    .unwrap()
}

/// A card with a second level that has a card mask, so the second level has a complement.
fn far_card() -> SceneMesh {
    let near = card(None).lods()[0].clone();
    let far = SceneMeshLod::new(
        1,
        near.vertices().to_vec(),
        near.indices().to_vec(),
        Some(BillboardPolicy::YawLocked),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::YawLocked, [2000, 2000], 500_000).unwrap())
    .unwrap();
    SceneMesh::with_distances(vec![near, far], vec![20_000, 200_000]).unwrap()
}

fn placed(mesh: u32, y: i64) -> SceneObject {
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0, y, 0],
            ..identity()
        },
        MeshHandle::from_raw(mesh),
        MaterialHandle::from_raw(0),
    );
    if mesh == 1 {
        object.with_skin_pose(SkinPoseHandle::from_raw(0))
    } else {
        object
    }
}

/// `rigid` (mesh 0) and the skinned ribbon (mesh 1) with two placements each, and a card with a
/// complement (mesh 2) that nothing places.
fn mixed(rigid: SceneMesh) -> RenderScene {
    let palette = vec![SceneAffineMatrix::IDENTITY, shifted(300)];
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new_with_skinning_resources(
            vec![rigid, ribbon(), far_card()],
            vec![
                placed(0, -1_200),
                placed(0, -1_300),
                placed(1, 1_200),
                placed(1, 1_300),
            ],
            vec![skeleton(2)],
            vec![SceneSkinPose::new(palette.clone(), palette).expect("two joints")],
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

fn upload(context: &GpuContext, scene: &RenderScene) -> SceneGpu {
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    SceneGpu::upload(context.device(), context.submission(), scene, &pipelines).unwrap()
}

/// The vertices of level `level` of mesh `mesh` as the upload packs them.
fn vertex_count(prepared: &super::super::PreparedScene, mesh: usize, level: usize) -> u32 {
    let lod = &prepared.meshes[mesh].lods[level];
    let stride = if lod.layout.is_skinned() { 64 } else { 56 };
    u32::try_from(lod.vertices.len() / stride).unwrap()
}

fn index_count(prepared: &super::super::PreparedScene, mesh: usize, level: usize) -> u32 {
    u32::try_from(prepared.meshes[mesh].lods[level].indices.len() / 4).unwrap()
}

/// The bytes the upload of `mesh` packs: vertices, indices and complements.
fn packed_bytes(mesh: &SceneMesh) -> u64 {
    let prepared = super::super::prepared::prepare_mesh(0, mesh).unwrap();
    prepared
        .lods
        .iter()
        .map(|lod| {
            lod.vertices.len() + lod.indices.len() + lod.transition.as_ref().map_or(0, Vec::len)
        })
        .sum::<usize>() as u64
}

fn mesh_census(context: &GpuContext) -> u64 {
    context
        .submission()
        .census()
        .snapshot()
        .get(ResourceClass::Mesh)
}

#[test]
fn the_levels_of_one_upload_share_a_buffer_for_each_vertex_format() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scene = mixed(rigid_ribbon());
    let gpu = upload(&context, &scene);
    let [rigid, skinned, card] = &gpu.meshes[..] else {
        panic!("three meshes were uploaded");
    };
    assert_eq!(rigid.lods[0].layout, GeometryLayout::Static);
    assert_eq!(skinned.lods[0].layout, GeometryLayout::Skinned);
    let complement = card.lods[1].transition.as_ref().expect("a card complement");
    let rigid_vertices = [
        &rigid.lods[0].vertices,
        &card.lods[0].vertices,
        complement,
        &card.lods[1].vertices,
    ];
    for vertices in &rigid_vertices[1..] {
        assert!(
            rigid_vertices[0].shares_chunk(vertices),
            "rigid vertices and complements share a chunk"
        );
    }
    assert!(
        !rigid_vertices[0].shares_chunk(&skinned.lods[0].vertices),
        "skinned vertices are another format, so another buffer"
    );
    let indices = [
        &rigid.lods[0].indices,
        &skinned.lods[0].indices,
        &card.lods[0].indices,
        &card.lods[1].indices,
    ];
    for span in &indices[1..] {
        assert!(
            indices[0].shares_chunk(span),
            "every level shares the indices"
        );
    }
}

#[test]
fn levels_sit_back_to_back_in_the_order_they_were_uploaded() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scene = mixed(rigid_ribbon());
    let prepared = super::super::PreparedScene::new(&scene).unwrap();
    let gpu = upload(&context, &scene);
    let [rigid, skinned, card] = &gpu.meshes[..] else {
        panic!("three meshes were uploaded");
    };
    // A level's complement is made before its vertices, and repacks the level before it.
    let (rigid_n, near_n) = (vertex_count(&prepared, 0, 0), vertex_count(&prepared, 2, 0));
    let firsts = [
        rigid.lods[0].vertices.first(),
        card.lods[0].vertices.first(),
        card.lods[1].transition.as_ref().unwrap().first(),
        card.lods[1].vertices.first(),
    ];
    assert_eq!(
        firsts,
        [0, rigid_n, rigid_n + near_n, rigid_n + 2 * near_n],
        "rigid vertices"
    );
    assert_eq!(skinned.lods[0].vertices.first(), 0, "skinned vertices");
    let indices = [
        rigid.lods[0].indices.first(),
        skinned.lods[0].indices.first(),
        card.lods[0].indices.first(),
        card.lods[1].indices.first(),
    ];
    let (a, b, c) = (
        index_count(&prepared, 0, 0),
        index_count(&prepared, 1, 0),
        index_count(&prepared, 2, 0),
    );
    assert_eq!(indices, [0, a, a + b, a + b + c], "indices");
    // The base vertex a draw adds is the level's first vertex.
    assert_eq!(
        card.lods[1].vertices.base_vertex(),
        i32::try_from(rigid_n + 2 * near_n).unwrap()
    );
}

#[test]
fn the_census_counts_the_chunks_and_a_replacement_leaves_only_the_bytes_it_replaced() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scene = mixed(rigid_ribbon());
    let before = mesh_census(&context);
    let mut gpu = upload(&context, &scene);
    let resident: u64 = scene.geometry().meshes().iter().map(packed_bytes).sum();
    assert_eq!(
        mesh_census(&context) - before,
        resident,
        "the chunks hold exactly the bytes the levels packed to"
    );
    let replace = |gpu: &mut SceneGpu, mesh: &SceneMesh| {
        let replacement =
            PreparedMeshReplacement::new(&scene, MeshHandle::from_raw(0), mesh).unwrap();
        gpu.replace_mesh_prepared(context.device(), context.queue(), replacement)
            .unwrap();
    };
    // The replaced level stays in the chunk its neighbours share, and the new one has a chunk.
    replace(&mut gpu, &quad());
    assert_eq!(
        mesh_census(&context) - before,
        resident + packed_bytes(&quad()),
        "after one replacement"
    );
    // The first replacement was alone in its chunk, which the second releases.
    replace(&mut gpu, &rigid_ribbon());
    assert_eq!(
        mesh_census(&context) - before,
        resident + packed_bytes(&rigid_ribbon()),
        "after two replacements"
    );
    drop(gpu);
    assert_eq!(
        mesh_census(&context),
        before,
        "dropping the scene frees every chunk"
    );
}

#[test]
fn a_replaced_mesh_draws_as_a_scene_made_with_it_and_leaves_the_others_alone() {
    let _session = crate::test_support::SceneTestSession::acquire();
    for ranged_only in [true, false] {
        let frame = unlit(pose());
        let mut rig = Rig::over(&mixed(rigid_ribbon()), ranged_only);
        let original = rig.render(&frame).pixels;
        let scene = mixed(rigid_ribbon());
        let replacement =
            PreparedMeshReplacement::new(&scene, MeshHandle::from_raw(0), &quad()).unwrap();
        rig.gpu
            .replace_mesh_prepared(rig.context.device(), rig.context.queue(), replacement)
            .unwrap();
        let replaced = rig.render(&frame).pixels;
        let made = Rig::over(&mixed(quad()), ranged_only).render(&frame).pixels;
        assert_ne!(replaced, original, "ranged_only {ranged_only}");
        assert_eq!(replaced, made, "ranged_only {ranged_only}");
    }
}

/// The colour attachment of `scene` from each of `poses`, on the route `ranged_only` picks,
/// uploaded with chunks of at most `limit` bytes.
fn frames(
    scene: &RenderScene,
    ranged_only: bool,
    limit: Option<u64>,
    poses: &[CameraPose],
) -> Vec<Vec<u8>> {
    let mut rig = match limit {
        Some(limit) => with_chunk_limit(limit, || Rig::over(scene, ranged_only)),
        None => Rig::over(scene, ranged_only),
    };
    poses
        .iter()
        .map(|pose| rig.render(&unlit(*pose)).pixels)
        .collect()
}

#[test]
fn shared_buffers_and_buffers_of_their_own_write_the_same_pixels_as_the_camera_moves() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let overhead = |x| CameraPose {
        eye_millimeters: [x, -2_000, 2_000],
        target_millimeters: [0, 0, 0],
    };
    let cases = [
        // Rigid and skinned levels, one scene of each pass.
        (
            mixed(rigid_ribbon()),
            [overhead(-1_500), overhead(0), overhead(1_500)],
        ),
        // Cards that change level with distance and draw a complement.
        (cards(), [pose_at(-3_000), pose_at(0), pose_at(3_000)]),
    ];
    for (index, (scene, poses)) in cases.iter().enumerate() {
        for ranged_only in [true, false] {
            let shared = frames(scene, ranged_only, None, poses);
            let separate = frames(scene, ranged_only, Some(1), poses);
            for (pose, (a, b)) in shared.iter().zip(&separate).enumerate() {
                assert!(lit_pixels(a) > 0, "scene {index} pose {pose} drew nothing");
                assert_eq!(a, b, "scene {index} pose {pose} ranged_only {ranged_only}");
            }
            assert!(
                shared.windows(2).any(|pair| pair[0] != pair[1]),
                "scene {index}: the camera moved and the frames did not change"
            );
        }
    }
}

/// `count` one-level cards in `count` meshes, all on material 0 and in view of [`pose_at`].
fn many_meshes(count: i64) -> RenderScene {
    let source = super::lod_material::scene(true);
    let meshes = (0..count).map(|_| card(None)).collect();
    let objects = (0..count)
        .map(|index| {
            SceneObject::new(
                SceneTransform {
                    position_millimeters: [-5_000 + 1_800 * index, 0, 0],
                    ..identity()
                },
                MeshHandle::from_raw(u32::try_from(index).unwrap()),
                MaterialHandle::from_raw(0),
            )
        })
        .collect();
    source
        .with_resources(
            source.resources().clone(),
            SceneGeometry::new(meshes, objects).unwrap(),
            None,
        )
        .unwrap()
}

/// What a compacted view of six meshes on one material records: the draws, the vertex and index
/// buffers it set, and its runs of draws that share every buffer.
fn recorded(limit: Option<u64>) -> [u32; 5] {
    let scene = many_meshes(6);
    let mut rig = match limit {
        Some(limit) => with_chunk_limit(limit, || Rig::over(&scene, false)),
        None => Rig::over(&scene, false),
    };
    rig.render(&unlit(pose_at(0)));
    assert!(rig.compacted_draws() > 0, "the view was compacted");
    let (tally, _) = rig.recorded();
    [
        tally.0.draws,
        tally.0.vertex_buffers,
        tally.0.index_buffers,
        tally.1.by_buffers,
        tally.1.by_state,
    ]
}

#[test]
fn meshes_that_share_a_chunk_are_one_run_of_draws_where_buffers_of_their_own_are_six() {
    let _session = crate::test_support::SceneTestSession::acquire();
    // Draws, vertex buffers set (the mesh's and the view's instances), index buffers set, runs
    // that share every buffer and runs that differ only in vertex and index buffers.
    assert_eq!(recorded(Some(1)), [6, 7, 6, 6, 1], "buffers of their own");
    assert_eq!(recorded(None), [6, 2, 1, 1, 1], "one chunk");
}
