use std::collections::{BTreeMap, BTreeSet};

use worldrunner_disunity_runtime::{MeshObjectView, MeshSlot};
use worldrunner_render_plan::{
    SceneMesh, SceneMeshLod, SceneTexture, SceneVertex, TexelFormat, TextureColorSpace,
};
use worldrunner_scene_viewer::{ObjectId, SceneObjectEdit};

use crate::scene_tables::{Changes, FrameContent, FrameUpdate, RESOURCE_INTERVAL, SceneTables};

#[derive(Default)]
struct Content {
    meshes: BTreeMap<u64, SceneMesh>,
    textures: BTreeMap<u64, SceneTexture>,
}

impl FrameContent for Content {
    fn mesh(&self, identity: u64) -> Option<&SceneMesh> {
        self.meshes.get(&identity)
    }
    fn texture(&self, identity: u64) -> Option<&SceneTexture> {
        self.textures.get(&identity)
    }
}

fn mesh(size: i64) -> SceneMesh {
    let vertices = [[0, 0, 0], [size, 0, 0], [0, size, 0]]
        .into_iter()
        .map(|position| SceneVertex {
            tangent_micro: None,
            position_millimeters: position,
            normal_micro: [0, 0, 1_000_000],
            texcoord_micro: [0, 0],
        })
        .collect();
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2], None).unwrap(),
    ])
    .unwrap()
}

fn texture(value: u8) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![value, value, value, 255].into(),
    }
}

fn content(meshes: &[u64]) -> Content {
    Content {
        meshes: meshes.iter().map(|id| (*id, mesh(1000))).collect(),
        textures: BTreeMap::new(),
    }
}

fn at(x: f32, y: f32, z: f32) -> [f32; 16] {
    [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., x, y, z, 1.]
}

fn object(object: u64, mesh: u64, matrix: [f32; 16]) -> MeshObjectView {
    MeshObjectView {
        object,
        mesh,
        local_to_world: matrix,
        slots: vec![MeshSlot {
            color: [1.0; 4],
            texture: 0,
        }],
    }
}

fn update(tables: &mut SceneTables, objects: &[MeshObjectView], content: &Content) -> FrameUpdate {
    tables
        .update(objects, content, &Changes::default())
        .expect("the tables stay valid")
}

fn adds(update: &FrameUpdate) -> Vec<ObjectId> {
    update
        .edits
        .iter()
        .filter_map(|edit| match edit {
            SceneObjectEdit::Add { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

fn replaces(update: &FrameUpdate) -> Vec<ObjectId> {
    update
        .edits
        .iter()
        .filter_map(|edit| match edit {
            SceneObjectEdit::Replace { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

fn removes(update: &FrameUpdate) -> Vec<ObjectId> {
    update
        .edits
        .iter()
        .filter_map(|edit| match edit {
            SceneObjectEdit::Remove(id) => Some(*id),
            _ => None,
        })
        .collect()
}

fn mesh_count(update: &FrameUpdate) -> usize {
    update
        .resources
        .as_ref()
        .map_or(0, |(_, meshes)| meshes.len())
}

/// Runs `count` frames of the same objects with no content changes.
fn frames(
    tables: &mut SceneTables,
    objects: &[MeshObjectView],
    content: &Content,
    count: u64,
) -> Vec<FrameUpdate> {
    (0..count)
        .map(|_| update(tables, objects, content))
        .collect()
}

/// Runs the frames up to the next resource send, `waited` frames after the last one, and returns
/// that frame's update. Every frame before it must send nothing.
fn until_sent(
    tables: &mut SceneTables,
    objects: &[MeshObjectView],
    content: &Content,
    waited: u64,
) -> FrameUpdate {
    let mut waiting = frames(tables, objects, content, RESOURCE_INTERVAL - 1 - waited);
    assert!(
        waiting.iter().all(|frame| frame.resources.is_none()),
        "no send before the interval has passed"
    );
    waiting.clear();
    update(tables, objects, content)
}

#[test]
fn the_first_frame_appends_its_meshes_and_adds_its_objects_in_one_update() {
    let mut tables = SceneTables::new().unwrap();
    let objects = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 100, at(1., 0., 0.)),
        object(9, 101, at(2., 0., 0.)),
    ];
    let first = update(&mut tables, &objects, &content(&[100, 101]));
    assert_eq!(
        mesh_count(&first),
        2,
        "two distinct meshes serve three objects"
    );
    let added = adds(&first);
    assert_eq!(added.len(), 3);
    assert_eq!(
        added.iter().collect::<BTreeSet<_>>().len(),
        3,
        "each object has its own id"
    );
    assert!(replaces(&first).is_empty() && removes(&first).is_empty());
}

#[test]
fn an_unchanged_frame_applies_nothing() {
    let mut tables = SceneTables::new().unwrap();
    let objects = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(1., 0., 0.)),
    ];
    let shown = content(&[100, 101]);
    update(&mut tables, &objects, &shown);
    let again = update(&mut tables, &objects, &shown);
    assert!(again.resources.is_none());
    assert!(again.edits.is_empty());
}

#[test]
fn a_moved_object_is_replaced_under_the_id_it_was_added_with() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let before = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 100, at(1., 0., 0.)),
    ];
    let added = adds(&update(&mut tables, &before, &shown));
    let after = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 100, at(5., 0., 0.)),
    ];
    let moved = update(&mut tables, &after, &shown);
    assert!(moved.resources.is_none());
    assert_eq!(replaces(&moved), vec![added[1]]);
    assert!(adds(&moved).is_empty() && removes(&moved).is_empty());
}

#[test]
fn a_vanished_object_is_removed_and_a_returning_one_is_added_again() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let both = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 100, at(1., 0., 0.)),
    ];
    let added = adds(&update(&mut tables, &both, &shown));
    let one = [object(7, 100, at(0., 0., 0.))];
    let gone = update(&mut tables, &one, &shown);
    assert_eq!(removes(&gone), vec![added[1]]);
    let back = update(&mut tables, &both, &shown);
    assert_eq!(adds(&back).len(), 1);
    assert!(removes(&back).is_empty());
}

#[test]
fn a_new_mesh_is_appended_after_the_existing_ones_without_touching_their_objects() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100, 101]);
    let first = [object(7, 100, at(0., 0., 0.))];
    update(&mut tables, &first, &shown);
    let second = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(1., 0., 0.)),
    ];
    let next = until_sent(&mut tables, &second, &shown, 0);
    assert_eq!(mesh_count(&next), 2);
    assert_eq!(adds(&next).len(), 1);
    assert!(replaces(&next).is_empty(), "object 7 kept its mesh handle");
}

#[test]
fn a_changed_mesh_replaces_its_table_entry_and_leaves_the_objects_alone() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    let objects = [object(7, 100, at(0., 0., 0.))];
    update(&mut tables, &objects, &shown);
    shown.meshes.insert(100, mesh(2000));
    let changed = Changes {
        meshes: [100].into(),
        textures: BTreeSet::new(),
    };
    let first_wait = tables.update(&objects, &shown, &changed).unwrap();
    assert!(first_wait.resources.is_none());
    let next = until_sent(&mut tables, &objects, &shown, 1);
    let (_, meshes) = next.resources.expect("the mesh table changed");
    assert_eq!(meshes.len(), 1);
    assert_eq!(
        meshes[0].lods()[0].vertices()[1].position_millimeters,
        [2000, 0, 0]
    );
    assert!(next.edits.is_empty());
}

#[test]
fn an_object_whose_mesh_is_not_converted_yet_waits_and_appears_when_it_is() {
    let mut tables = SceneTables::new().unwrap();
    let objects = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(1., 0., 0.)),
    ];
    let early = update(&mut tables, &objects, &content(&[100]));
    assert_eq!(adds(&early).len(), 1, "only the object whose mesh exists");
    let later = until_sent(&mut tables, &objects, &content(&[100, 101]), 0);
    assert_eq!(adds(&later).len(), 1);
    assert_eq!(mesh_count(&later), 2);
}

#[test]
fn an_object_with_a_matrix_the_scene_cannot_place_is_reported_and_left_out() {
    let mut tables = SceneTables::new().unwrap();
    let mut sheared = at(0., 0., 0.);
    sheared[4] = 0.5;
    let objects = [object(7, 100, at(0., 0., 0.)), object(8, 100, sheared)];
    let first = update(&mut tables, &objects, &content(&[100]));
    assert_eq!(adds(&first).len(), 1);
    assert_eq!(first.skipped.len(), 1);
    assert_eq!(first.skipped[0].object, 8);
}

#[test]
fn an_object_that_becomes_unplaceable_is_removed() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let added = adds(&update(
        &mut tables,
        &[object(7, 100, at(0., 0., 0.))],
        &shown,
    ));
    let mut flat = at(0., 0., 0.);
    flat[0] = 0.0;
    flat[5] = 0.0;
    flat[10] = 0.0;
    let next = update(&mut tables, &[object(7, 100, flat)], &shown);
    assert_eq!(removes(&next), added);
    assert_eq!(next.skipped.len(), 1);
}

fn slot(color: [f32; 4], texture: u64) -> MeshSlot {
    MeshSlot { color, texture }
}

fn with_slot(mut view: MeshObjectView, slot: MeshSlot) -> MeshObjectView {
    view.slots = vec![slot];
    view
}

fn material_of(update: &FrameUpdate, id: ObjectId) -> u32 {
    update
        .edits
        .iter()
        .find_map(|edit| match edit {
            SceneObjectEdit::Add { id: at, object }
            | SceneObjectEdit::Replace { id: at, object }
                if *at == id =>
            {
                Some(object.material().raw())
            }
            _ => None,
        })
        .expect("the object was edited")
}

fn tint_of(update: &FrameUpdate, id: ObjectId) -> [u32; 3] {
    update
        .edits
        .iter()
        .find_map(|edit| match edit {
            SceneObjectEdit::Add { id: at, object }
            | SceneObjectEdit::Replace { id: at, object }
                if *at == id =>
            {
                Some(object.tint().channels())
            }
            _ => None,
        })
        .expect("the object was edited")
}

#[test]
fn the_slot_colour_tints_the_object_and_the_slot_texture_picks_the_material() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    shown.textures.insert(5, texture(200));
    let red = with_slot(object(1, 100, at(0., 0., 0.)), slot([1., 0.5, 0., 1.], 0));
    let also_red = with_slot(object(2, 100, at(1., 0., 0.)), slot([1., 0.5, 0., 1.], 0));
    let blue = with_slot(object(3, 100, at(2., 0., 0.)), slot([0., 0., 1., 1.], 0));
    let textured = with_slot(object(4, 100, at(3., 0., 0.)), slot([1.; 4], 5));
    let first = update(&mut tables, &[red, also_red, blue, textured], &shown);
    let ids = adds(&first);
    assert_eq!(tint_of(&first, ids[0]), [1_000_000, 500_000, 0]);
    assert_eq!(tint_of(&first, ids[2]), [0, 0, 1_000_000]);
    assert_eq!(tint_of(&first, ids[3]), [1_000_000; 3]);
    let untextured = material_of(&first, ids[0]);
    assert_eq!(material_of(&first, ids[1]), untextured);
    assert_eq!(material_of(&first, ids[2]), untextured);
    assert_ne!(
        material_of(&first, ids[3]),
        untextured,
        "a textured slot draws with its own material"
    );
    let (resources, _) = first.resources.expect("a texture and a material are new");
    assert_eq!(
        resources.textures().len(),
        2,
        "the white texture and texture 5"
    );
    assert_eq!(resources.materials().len(), 2);
}

#[test]
fn a_recoloured_object_is_replaced_and_adds_no_material() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let before = with_slot(object(1, 100, at(0., 0., 0.)), slot([1.; 4], 0));
    let id = adds(&update(&mut tables, &[before], &shown))[0];
    let after = with_slot(object(1, 100, at(0., 0., 0.)), slot([0.2, 0.2, 0.2, 1.], 0));
    let next = update(&mut tables, &[after], &shown);
    assert!(next.resources.is_none());
    assert_eq!(replaces(&next), vec![id]);
}

#[test]
fn a_texture_that_has_no_texels_yet_draws_white_until_it_arrives() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    let view = with_slot(object(1, 100, at(0., 0., 0.)), slot([1.; 4], 5));
    let early = update(&mut tables, std::slice::from_ref(&view), &shown);
    let id = adds(&early)[0];
    assert_eq!(material_of(&early, id), 0, "the white material");
    shown.textures.insert(5, texture(90));
    let changed = Changes {
        meshes: BTreeSet::new(),
        textures: [5].into(),
    };
    let first_wait = tables
        .update(std::slice::from_ref(&view), &shown, &changed)
        .unwrap();
    assert!(first_wait.resources.is_none() && first_wait.edits.is_empty());
    let later = until_sent(&mut tables, &[view], &shown, 1);
    assert_eq!(replaces(&later), vec![id]);
    assert_ne!(material_of(&later, id), 0);
    assert_eq!(
        later
            .resources
            .expect("textures changed")
            .0
            .textures()
            .len(),
        2
    );
}

#[test]
fn an_object_without_material_slots_draws_white_and_untinted() {
    let mut tables = SceneTables::new().unwrap();
    let mut view = object(1, 100, at(0., 0., 0.));
    view.slots.clear();
    let first = update(&mut tables, &[view], &content(&[100]));
    let id = adds(&first)[0];
    assert_eq!(material_of(&first, id), 0);
    assert_eq!(tint_of(&first, id), [1_000_000; 3]);
}

#[test]
fn a_colour_above_one_or_below_zero_stays_within_the_tint_range() {
    let mut tables = SceneTables::new().unwrap();
    let view = with_slot(
        object(1, 100, at(0., 0., 0.)),
        slot([9., -3., f32::NAN, 1.], 0),
    );
    let first = update(&mut tables, &[view], &content(&[100]));
    let id = adds(&first)[0];
    assert_eq!(tint_of(&first, id), [4_000_000, 0, 0]);
}

#[test]
fn the_initial_scene_is_valid_before_any_frame_arrives() {
    let tables = SceneTables::new().unwrap();
    let scene = tables.initial_scene().unwrap();
    assert!(scene.geometry().objects().is_empty());
    assert_eq!(scene.resources().textures().len(), 1);
    assert_eq!(scene.resources().materials().len(), 1);
}

#[test]
fn objects_sharing_a_texture_share_one_material_across_frames() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    shown.textures.insert(5, texture(200));
    let two = |x: f32| {
        [
            with_slot(object(1, 100, at(x, 0., 0.)), slot([1.; 4], 5)),
            with_slot(object(2, 100, at(x, 1., 0.)), slot([0.5; 4], 5)),
        ]
    };
    let first = update(&mut tables, &two(0.), &shown);
    let ids = adds(&first);
    assert_eq!(material_of(&first, ids[0]), material_of(&first, ids[1]));
    assert_eq!(
        first
            .resources
            .as_ref()
            .expect("new texture")
            .0
            .materials()
            .len(),
        2
    );
    let moved = update(&mut tables, &two(3.), &shown);
    assert!(moved.resources.is_none(), "no further material is built");
    assert_eq!(material_of(&moved, ids[0]), material_of(&first, ids[0]));
}

fn moved_pair(second_x: f32) -> [MeshObjectView; 2] {
    [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(second_x, 0., 0.)),
    ]
}

#[test]
fn the_first_frame_with_content_sends_the_resources_at_once() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let objects = [object(7, 100, at(0., 0., 0.))];
    let empty = frames(&mut tables, &[], &shown, 3);
    assert!(empty.iter().all(|frame| frame.resources.is_none()));
    let first = update(&mut tables, &objects, &shown);
    assert_eq!(mesh_count(&first), 1);
    assert_eq!(adds(&first).len(), 1);
}

#[test]
fn a_new_mesh_waits_for_the_interval_and_its_objects_appear_with_it() {
    let mut tables = SceneTables::new().unwrap();
    update(
        &mut tables,
        &[object(7, 100, at(0., 0., 0.))],
        &content(&[100]),
    );
    let both = moved_pair(1.);
    let shown = content(&[100, 101]);
    let waiting = frames(&mut tables, &both, &shown, RESOURCE_INTERVAL - 1);
    for frame in &waiting {
        assert!(frame.resources.is_none(), "the new mesh is not sent yet");
        assert!(adds(frame).is_empty(), "its object is held back");
        assert!(frame.skipped.is_empty());
    }
    let sent = update(&mut tables, &both, &shown);
    assert_eq!(mesh_count(&sent), 2);
    assert_eq!(adds(&sent).len(), 1, "the object appears with its mesh");
    let after = update(&mut tables, &both, &shown);
    assert!(after.resources.is_none() && after.edits.is_empty());
}

#[test]
fn a_send_carries_every_mesh_that_arrived_while_it_waited() {
    let mut tables = SceneTables::new().unwrap();
    update(
        &mut tables,
        &[object(7, 100, at(0., 0., 0.))],
        &content(&[100]),
    );
    let three = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(1., 0., 0.)),
        object(9, 102, at(2., 0., 0.)),
    ];
    frames(&mut tables, &three[..2], &content(&[100, 101]), 2);
    frames(
        &mut tables,
        &three,
        &content(&[100, 101, 102]),
        RESOURCE_INTERVAL - 3,
    );
    let sent = update(&mut tables, &three, &content(&[100, 101, 102]));
    assert_eq!(mesh_count(&sent), 3);
    assert_eq!(adds(&sent).len(), 2);
}

#[test]
fn the_interval_counts_from_the_last_send() {
    let mut tables = SceneTables::new().unwrap();
    update(
        &mut tables,
        &[object(7, 100, at(0., 0., 0.))],
        &content(&[100]),
    );
    let both = moved_pair(1.);
    let shown = content(&[100, 101]);
    until_sent(&mut tables, &both, &shown, 0);
    let three = [
        both[0].clone(),
        both[1].clone(),
        object(9, 102, at(2., 0., 0.)),
    ];
    let all = content(&[100, 101, 102]);
    let next = until_sent(&mut tables, &three, &all, 0);
    assert_eq!(mesh_count(&next), 3);
}

#[test]
fn a_moved_object_is_edited_at_once_while_resources_wait() {
    let mut tables = SceneTables::new().unwrap();
    let first = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 100, at(1., 0., 0.)),
    ];
    let added = adds(&update(&mut tables, &first, &content(&[100])));
    let shown = content(&[100, 101]);
    let waiting = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 100, at(5., 0., 0.)),
        object(9, 101, at(2., 0., 0.)),
    ];
    let next = update(&mut tables, &waiting, &shown);
    assert!(next.resources.is_none());
    assert_eq!(replaces(&next), vec![added[1]]);
    assert!(adds(&next).is_empty() && removes(&next).is_empty());
}

#[test]
fn an_object_placed_earlier_keeps_its_old_state_while_its_new_texture_waits() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    let plain = with_slot(object(1, 100, at(0., 0., 0.)), slot([1.; 4], 5));
    let id = adds(&update(&mut tables, std::slice::from_ref(&plain), &shown))[0];
    shown.textures.insert(5, texture(90));
    let changed = Changes {
        meshes: BTreeSet::new(),
        textures: [5].into(),
    };
    let moved = with_slot(object(1, 100, at(3., 0., 0.)), slot([1.; 4], 5));
    let waiting = tables
        .update(std::slice::from_ref(&moved), &shown, &changed)
        .unwrap();
    assert!(waiting.resources.is_none());
    assert!(waiting.edits.is_empty(), "not removed, not replaced");
    assert!(waiting.skipped.is_empty());
    let sent = until_sent(&mut tables, &[moved], &shown, 1);
    assert_eq!(replaces(&sent), vec![id]);
    assert_eq!(sent.resources.expect("sent").0.textures().len(), 2);
}

#[test]
fn a_changed_mesh_is_sent_with_its_latest_contents_on_the_next_send() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    let objects = [object(7, 100, at(0., 0., 0.))];
    update(&mut tables, &objects, &shown);
    shown.meshes.insert(100, mesh(2000));
    let changed = Changes {
        meshes: [100].into(),
        textures: BTreeSet::new(),
    };
    let wait = tables.update(&objects, &shown, &changed).unwrap();
    assert!(wait.resources.is_none());
    shown.meshes.insert(100, mesh(3000));
    let sent = until_sent(&mut tables, &objects, &shown, 1);
    let (_, meshes) = sent.resources.expect("the changed mesh is sent");
    assert_eq!(
        meshes[0].lods()[0].vertices()[1].position_millimeters,
        [3000, 0, 0]
    );
    assert!(sent.edits.is_empty());
}

#[test]
fn nothing_is_sent_while_nothing_changes() {
    let mut tables = SceneTables::new().unwrap();
    let objects = moved_pair(1.);
    let shown = content(&[100, 101]);
    update(&mut tables, &objects, &shown);
    for frame in frames(&mut tables, &objects, &shown, 3 * RESOURCE_INTERVAL) {
        assert!(frame.resources.is_none() && frame.edits.is_empty());
    }
}

#[test]
fn objects_held_back_for_their_mesh_are_not_reported_as_skipped() {
    let mut tables = SceneTables::new().unwrap();
    update(
        &mut tables,
        &[object(7, 100, at(0., 0., 0.))],
        &content(&[100]),
    );
    let both = moved_pair(1.);
    let waiting = frames(&mut tables, &both, &content(&[100, 101]), 2);
    assert!(waiting.iter().all(|frame| frame.skipped.is_empty()));
    assert!(waiting.iter().all(|frame| frame.edits.is_empty()));
}
