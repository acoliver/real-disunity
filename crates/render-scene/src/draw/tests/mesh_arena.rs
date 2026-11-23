//! Mesh levels share buffers: where they sit when an arena has chunks of a given size, what they
//! cost in the memory census, and that a scene whose levels roll over into several chunks draws
//! the pixels of the scene whose levels share one.
use super::compaction::{Rig, cards, pose_at, unlit};
use super::skinning::{ribbon, shifted, skeleton};
use super::*;
use crate::draw::PreparedMeshReplacement;
use crate::draw::mesh_arena::{MAX_CHUNK_BYTES, MeshArena};
use crate::object::layout::GeometryLayout;
use crate::test_support::upload_scene;
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
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    upload_scene(context.device(), context.submission(), scene, &pipelines).unwrap()
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

/// `rig`'s meshes uploaded again by an arena of `chunk_bytes`, before anything draws.
fn rechunk(rig: &mut Rig, scene: &RenderScene, chunk_bytes: u64) {
    let prepared = super::super::PreparedScene::new(scene).unwrap();
    rig.gpu.arena = MeshArena::new(chunk_bytes);
    rig.gpu.meshes = rig.gpu.arena.upload(
        rig.context.device(),
        rig.context.submission().census(),
        &prepared.meshes,
    );
    rig.gpu.retained.lent_meshes.get_mut().take();
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
            rigid_vertices[0].buffer() == vertices.buffer(),
            "rigid vertices and complements share a chunk"
        );
    }
    assert!(
        rigid_vertices[0].buffer() != skinned.lods[0].vertices.buffer(),
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
            indices[0].buffer() == span.buffer(),
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
        gpu.replace_mesh_prepared(context.device(), replacement)
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
    for (ranged_only, chunk_bytes) in [(true, MAX_CHUNK_BYTES), (false, 1)] {
        let frame = unlit(pose());
        let mut rig = Rig::over(&mixed(rigid_ribbon()), ranged_only);
        rig.gpu.arena = MeshArena::new(chunk_bytes);
        let original = rig.render(&frame).pixels;
        let scene = mixed(rigid_ribbon());
        let replacement =
            PreparedMeshReplacement::new(&scene, MeshHandle::from_raw(0), &quad()).unwrap();
        rig.gpu
            .replace_mesh_prepared(rig.context.device(), replacement)
            .unwrap();
        let replaced = rig.render(&frame).pixels;
        let made = Rig::over(&mixed(quad()), ranged_only).render(&frame).pixels;
        assert_ne!(replaced, original, "chunks of {chunk_bytes} bytes");
        assert_eq!(replaced, made, "chunks of {chunk_bytes} bytes");
    }
}

/// The colour attachment of `scene` from each of `poses`, on the route `ranged_only` picks, with
/// the meshes in chunks of `chunk_bytes`.
fn frames(
    scene: &RenderScene,
    ranged_only: bool,
    chunk_bytes: u64,
    poses: &[CameraPose],
) -> Vec<Vec<u8>> {
    let mut rig = Rig::over(scene, ranged_only);
    rechunk(&mut rig, scene, chunk_bytes);
    poses
        .iter()
        .map(|pose| rig.render(&unlit(*pose)).pixels)
        .collect()
}

#[test]
fn meshes_split_across_chunks_write_the_pixels_of_meshes_in_one_chunk_as_the_camera_moves() {
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
            let whole = frames(scene, ranged_only, MAX_CHUNK_BYTES, poses);
            // One level to a chunk, then chunks that hold a few levels each.
            for chunk_bytes in [1, 300, 1_000] {
                let split = frames(scene, ranged_only, chunk_bytes, poses);
                for (pose, (a, b)) in whole.iter().zip(&split).enumerate() {
                    assert!(lit_pixels(a) > 0, "scene {index} pose {pose} drew nothing");
                    assert_eq!(
                        a, b,
                        "scene {index} pose {pose} ranged_only {ranged_only} chunks of {chunk_bytes}"
                    );
                }
            }
            assert!(
                whole.windows(2).any(|pair| pair[0] != pair[1]),
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

/// What a compacted view of six meshes on one material records, with the meshes in chunks of
/// `chunk_bytes`: the draws, the vertex and index buffers it set, and its runs of draws that
/// share every buffer.
fn recorded(chunk_bytes: u64) -> [u32; 5] {
    let scene = many_meshes(6);
    let mut rig = Rig::over(&scene, false);
    rechunk(&mut rig, &scene, chunk_bytes);
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
fn meshes_in_one_chunk_are_one_run_of_draws_and_meshes_in_chunks_of_their_own_are_six() {
    let _session = crate::test_support::SceneTestSession::acquire();
    // Draws, vertex buffers set (the mesh's and the view's instances), index buffers set, runs
    // that share every buffer and runs that differ only in vertex and index buffers.
    assert_eq!(recorded(1), [6, 7, 6, 6, 1], "a chunk to each level");
    assert_eq!(recorded(MAX_CHUNK_BYTES), [6, 2, 1, 1, 1], "one chunk");
}

#[test]
fn a_chunk_that_fills_continues_in_a_new_chunk_whose_levels_start_at_its_first_vertex() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scene = mixed(rigid_ribbon());
    let prepared = super::super::PreparedScene::new(&scene).unwrap();
    let (rigid_n, near_n) = (vertex_count(&prepared, 0, 0), vertex_count(&prepared, 2, 0));
    // Static vertices pack as the rigid ribbon, the near card, the far card's complement and the
    // far card. Room for the first two fills a chunk, and the rest start another.
    let arena = MeshArena::new(u64::from(rigid_n + near_n) * 56);
    let meshes = arena.upload(
        context.device(),
        context.submission().census(),
        &prepared.meshes,
    );
    let [rigid, _, card] = &meshes[..] else {
        panic!("three meshes were uploaded");
    };
    let complement = card.lods[1].transition.as_ref().unwrap();
    let (rigid_v, near_v) = (&rigid.lods[0].vertices, &card.lods[0].vertices);
    let far_v = &card.lods[1].vertices;
    assert!(
        rigid_v.buffer() == near_v.buffer(),
        "the first two fill a chunk"
    );
    assert!(
        complement.buffer() == far_v.buffer(),
        "the last two start another"
    );
    assert!(near_v.buffer() != complement.buffer(), "the chunk filled");
    assert_eq!(
        [
            rigid_v.first(),
            near_v.first(),
            complement.first(),
            far_v.first()
        ],
        [0, rigid_n, 0, near_n],
        "a new chunk starts at its own first vertex"
    );
    assert_eq!(far_v.base_vertex(), i32::try_from(near_n).unwrap());
}

#[test]
fn a_level_larger_than_a_chunk_has_a_chunk_of_its_own() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scene = mixed(rigid_ribbon());
    let prepared = super::super::PreparedScene::new(&scene).unwrap();
    let meshes = MeshArena::new(1).upload(
        context.device(),
        context.submission().census(),
        &prepared.meshes,
    );
    let spans: Vec<_> = meshes
        .iter()
        .flat_map(|mesh| &mesh.lods)
        .flat_map(|lod| {
            [
                Some(&lod.vertices),
                lod.transition.as_ref(),
                Some(&lod.indices),
            ]
        })
        .flatten()
        .collect();
    for (at, span) in spans.iter().enumerate() {
        assert_eq!(span.first(), 0, "span {at} starts its chunk");
        for other in &spans[at + 1..] {
            assert!(
                span.buffer() != other.buffer(),
                "span {at} has its chunk to itself"
            );
        }
    }
}

#[test]
fn the_census_counts_chunks_of_every_size_by_the_bytes_the_levels_pack_to() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scene = mixed(rigid_ribbon());
    let prepared = super::super::PreparedScene::new(&scene).unwrap();
    let resident: u64 = scene.geometry().meshes().iter().map(packed_bytes).sum();
    for chunk_bytes in [1, 300, 1_000, MAX_CHUNK_BYTES] {
        let before = mesh_census(&context);
        let meshes = MeshArena::new(chunk_bytes).upload(
            context.device(),
            context.submission().census(),
            &prepared.meshes,
        );
        assert_eq!(
            mesh_census(&context) - before,
            resident,
            "chunks of {chunk_bytes} bytes"
        );
        drop(meshes);
        assert_eq!(mesh_census(&context), before, "dropping frees every chunk");
    }
}

#[test]
fn levels_lent_from_later_chunks_name_the_chunk_and_where_they_start_in_it() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let scene = mixed(rigid_ribbon());
    let prepared = super::super::PreparedScene::new(&scene).unwrap();
    let (rigid_n, near_n) = (vertex_count(&prepared, 0, 0), vertex_count(&prepared, 2, 0));
    let mut rig = Rig::over(&scene, false);
    rechunk(&mut rig, &scene, u64::from(rigid_n + near_n) * 56);
    let lent = rig.gpu.lent_meshes();
    let (rigid, card) = (&lent[0].levels[0], &lent[2]);
    assert_eq!(rigid.base_vertex, 0);
    assert_eq!(card.levels[0].vertices, rigid.vertices, "the first chunk");
    assert_eq!(card.levels[0].base_vertex, i32::try_from(rigid_n).unwrap());
    assert_ne!(card.levels[1].vertices, rigid.vertices, "the second chunk");
    assert_eq!(card.levels[1].base_vertex, i32::try_from(near_n).unwrap());
}
