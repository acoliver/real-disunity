use std::collections::{BTreeMap, BTreeSet};

use worldrunner_disunity_runtime::{MeshObjectView, MeshSlot};
use worldrunner_render_plan::{
    SceneMesh, SceneMeshLod, SceneTexture, SceneVertex, TexelFormat, TextureColorSpace,
};
use worldrunner_scene_viewer::{ObjectId, SceneObjectEdit};

use crate::retained_driver::{PlayerWindow, viewer_options};
use crate::scene_tables::{Changes, FrameContent, FrameUpdate, SceneTables};

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
    tables.update(objects, content, &Changes::default())
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

fn appended_meshes(update: &FrameUpdate) -> usize {
    update
        .append
        .as_ref()
        .map_or(0, |append| append.meshes.len())
}

fn appended_textures(update: &FrameUpdate) -> usize {
    update
        .append
        .as_ref()
        .map_or(0, |append| append.textures.len())
}

fn appended_materials(update: &FrameUpdate) -> usize {
    update
        .append
        .as_ref()
        .map_or(0, |append| append.materials.len())
}

/// Whether the frame sends no resource of any kind.
fn sends_no_resources(update: &FrameUpdate) -> bool {
    update.append.is_none()
        && update.replaced_meshes.is_empty()
        && update.replaced_textures.is_empty()
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
        appended_meshes(&first),
        2,
        "two distinct meshes serve three objects"
    );
    assert_eq!(appended_textures(&first), 0);
    assert_eq!(
        appended_materials(&first),
        0,
        "the white material serves them"
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
    assert!(sends_no_resources(&again));
    assert!(again.edits.is_empty());
    assert!(again.into_updates().is_empty());
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
    assert!(sends_no_resources(&moved));
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

fn mesh_of(update: &FrameUpdate, id: ObjectId) -> u32 {
    update
        .edits
        .iter()
        .find_map(|edit| match edit {
            SceneObjectEdit::Add { id: at, object }
            | SceneObjectEdit::Replace { id: at, object }
                if *at == id =>
            {
                Some(object.mesh().raw())
            }
            _ => None,
        })
        .expect("the object was edited")
}

#[test]
fn a_new_mesh_is_appended_in_the_frame_that_shows_it_after_the_existing_ones() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100, 101]);
    let first = [object(7, 100, at(0., 0., 0.))];
    update(&mut tables, &first, &shown);
    let second = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(1., 0., 0.)),
    ];
    let next = update(&mut tables, &second, &shown);
    assert_eq!(appended_meshes(&next), 1);
    assert_eq!(adds(&next).len(), 1, "its object appears with it");
    assert_eq!(mesh_of(&next, adds(&next)[0]), 1, "the next mesh handle");
    assert!(replaces(&next).is_empty(), "object 7 kept its mesh handle");
}

#[test]
fn mesh_handles_are_never_reused_across_frames() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100, 101, 102]);
    let mut handles = Vec::new();
    for (frame, mesh) in [100, 101, 102].into_iter().enumerate() {
        let objects: Vec<_> = [100, 101, 102]
            .into_iter()
            .take(frame + 1)
            .enumerate()
            .map(|(at_index, id)| object(at_index as u64, id, at(0., 0., 0.)))
            .collect();
        let next = update(&mut tables, &objects, &shown);
        assert_eq!(appended_meshes(&next), 1, "mesh {mesh} is the only new one");
        handles.push(mesh_of(&next, adds(&next)[0]));
    }
    assert_eq!(handles, [0, 1, 2]);
}

#[test]
fn a_changed_mesh_is_replaced_by_handle_and_leaves_the_objects_alone() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100, 101]);
    let objects = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(1., 0., 0.)),
    ];
    update(&mut tables, &objects, &shown);
    shown.meshes.insert(101, mesh(2000));
    let changed = Changes {
        meshes: [101].into(),
        textures: BTreeSet::new(),
    };
    let next = tables.update(&objects, &shown, &changed);
    assert!(next.append.is_none());
    assert_eq!(next.replaced_meshes.len(), 1);
    let (handle, replaced) = &next.replaced_meshes[0];
    assert_eq!(handle.raw(), 1, "the handle of the second mesh");
    assert_eq!(
        replaced.lods()[0].vertices()[1].position_millimeters,
        [2000, 0, 0]
    );
    assert!(next.edits.is_empty());
    let after = update(&mut tables, &objects, &shown);
    assert!(sends_no_resources(&after), "the change is sent once");
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
    let later = update(&mut tables, &objects, &content(&[100, 101]));
    assert_eq!(adds(&later).len(), 1);
    assert_eq!(appended_meshes(&later), 1);
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
    assert_eq!(
        appended_textures(&first),
        1,
        "texture 5 follows the white one"
    );
    assert_eq!(appended_materials(&first), 1);
    assert_eq!(material_of(&first, ids[3]), 1, "the next material handle");
}

#[test]
fn a_recoloured_object_is_replaced_and_adds_no_material() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let before = with_slot(object(1, 100, at(0., 0., 0.)), slot([1.; 4], 0));
    let id = adds(&update(&mut tables, &[before], &shown))[0];
    let after = with_slot(object(1, 100, at(0., 0., 0.)), slot([0.2, 0.2, 0.2, 1.], 0));
    let next = update(&mut tables, &[after], &shown);
    assert!(sends_no_resources(&next));
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
    let later = tables.update(std::slice::from_ref(&view), &shown, &changed);
    assert_eq!(replaces(&later), vec![id]);
    assert_ne!(material_of(&later, id), 0);
    assert_eq!(appended_textures(&later), 1);
    assert_eq!(appended_materials(&later), 1);
    assert!(
        later.replaced_textures.is_empty(),
        "it was appended, not replaced"
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
    let scene = SceneTables::initial_scene(None).unwrap();
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
    assert_eq!(appended_materials(&first), 1);
    let moved = update(&mut tables, &two(3.), &shown);
    assert!(sends_no_resources(&moved), "no further material is built");
    assert_eq!(material_of(&moved, ids[0]), material_of(&first, ids[0]));
}

fn moved_pair(second_x: f32) -> [MeshObjectView; 2] {
    [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(second_x, 0., 0.)),
    ]
}

#[test]
fn frames_without_objects_send_nothing() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let empty = frames(&mut tables, &[], &shown, 3);
    assert!(empty.iter().all(sends_no_resources));
    assert!(empty.iter().all(|frame| frame.edits.is_empty()));
}

#[test]
fn every_mesh_that_arrived_in_a_frame_is_appended_in_that_frame() {
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
    let sent = update(&mut tables, &three, &content(&[100, 101, 102]));
    assert_eq!(appended_meshes(&sent), 2);
    assert_eq!(adds(&sent).len(), 2);
}

#[test]
fn a_moved_object_and_a_new_mesh_share_one_update() {
    let mut tables = SceneTables::new().unwrap();
    let first = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 100, at(1., 0., 0.)),
    ];
    let added = adds(&update(&mut tables, &first, &content(&[100])));
    let shown = content(&[100, 101]);
    let next = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 100, at(5., 0., 0.)),
        object(9, 101, at(2., 0., 0.)),
    ];
    let next = update(&mut tables, &next, &shown);
    assert_eq!(appended_meshes(&next), 1);
    assert_eq!(replaces(&next), vec![added[1]]);
    assert_eq!(adds(&next).len(), 1);
}

#[test]
fn a_changed_mesh_is_sent_with_its_latest_contents() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    let objects = [object(7, 100, at(0., 0., 0.))];
    update(&mut tables, &objects, &shown);
    shown.meshes.insert(100, mesh(2000));
    let changed = Changes {
        meshes: [100].into(),
        textures: BTreeSet::new(),
    };
    let first = tables.update(&objects, &shown, &changed);
    assert_eq!(first.replaced_meshes.len(), 1);
    shown.meshes.insert(100, mesh(3000));
    let second = tables.update(&objects, &shown, &changed);
    let (_, latest) = &second.replaced_meshes[0];
    assert_eq!(
        latest.lods()[0].vertices()[1].position_millimeters,
        [3000, 0, 0]
    );
}

#[test]
fn a_changed_texture_is_replaced_by_handle_with_its_latest_texels() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    shown.textures.insert(5, texture(10));
    let view = with_slot(object(1, 100, at(0., 0., 0.)), slot([1.; 4], 5));
    let objects = [view];
    update(&mut tables, &objects, &shown);
    shown.textures.insert(5, texture(77));
    let changed = Changes {
        meshes: BTreeSet::new(),
        textures: [5].into(),
    };
    let sent = tables.update(&objects, &shown, &changed);
    assert!(sent.append.is_none());
    assert_eq!(sent.replaced_textures.len(), 1);
    let (handle, replaced) = &sent.replaced_textures[0];
    assert_eq!(handle.raw(), 1, "the handle after the white texture");
    assert_eq!(*replaced, texture(77));
    assert!(sent.edits.is_empty());
    for frame in frames(&mut tables, &objects, &shown, 3) {
        assert!(sends_no_resources(&frame), "the change is sent once");
    }
}

#[test]
fn a_change_to_something_the_viewer_never_received_is_not_sent() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let objects = [object(7, 100, at(0., 0., 0.))];
    update(&mut tables, &objects, &shown);
    let changed = Changes {
        meshes: [555].into(),
        textures: [666].into(),
    };
    let next = tables.update(&objects, &shown, &changed);
    assert!(sends_no_resources(&next));
}

#[test]
fn nothing_is_sent_while_nothing_changes() {
    let mut tables = SceneTables::new().unwrap();
    let objects = moved_pair(1.);
    let shown = content(&[100, 101]);
    update(&mut tables, &objects, &shown);
    for frame in frames(&mut tables, &objects, &shown, 24) {
        assert!(sends_no_resources(&frame) && frame.edits.is_empty());
    }
}

#[test]
fn the_updates_go_out_as_appends_then_replacements_then_objects() {
    use worldrunner_scene_viewer::RetainedUpdate;
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    shown.textures.insert(5, texture(10));
    let first = with_slot(object(1, 100, at(0., 0., 0.)), slot([1.; 4], 5));
    update(&mut tables, std::slice::from_ref(&first), &shown);
    shown.meshes.insert(100, mesh(2000));
    shown.meshes.insert(101, mesh(1000));
    shown.textures.insert(5, texture(77));
    let changed = Changes {
        meshes: [100].into(),
        textures: [5].into(),
    };
    let both = [first, object(2, 101, at(1., 0., 0.))];
    let updates = tables.update(&both, &shown, &changed).into_updates();
    assert!(matches!(
        updates.as_slice(),
        [
            RetainedUpdate::AppendResources(_),
            RetainedUpdate::ReplaceTexture { .. },
            RetainedUpdate::ReplaceMesh { .. },
            RetainedUpdate::Objects(_)
        ]
    ));
}

#[test]
fn the_viewer_accepts_every_frame_the_tables_produce() {
    use worldrunner_scene_viewer::{RetainedHost, ViewerOptions};
    let mut tables = SceneTables::new().unwrap();
    let window = PlayerWindow {
        title: "tables".into(),
        size: [64, 64],
        fixed_dt: None,
        max_frames: None,
        capture_frames: BTreeSet::new(),
        keys: Vec::new(),
    };
    let options: ViewerOptions = viewer_options(&window);
    let mut viewer = RetainedHost::new(SceneTables::initial_scene(None).unwrap(), options).unwrap();
    let mut shown = content(&[100]);
    shown.textures.insert(5, texture(10));
    let mut objects = vec![with_slot(object(1, 100, at(0., 0., 0.)), slot([1.; 4], 5))];
    for frame in 0..6_u32 {
        let id = u64::from(frame);
        shown.meshes.insert(200 + id, mesh(1000 + i64::from(frame)));
        objects.push(object(
            10 + id,
            200 + id,
            at(f32::from(u16::try_from(frame).unwrap()), 0., 0.),
        ));
        let changed = Changes {
            meshes: [100].into(),
            textures: if frame == 3 {
                [5].into()
            } else {
                BTreeSet::new()
            },
        };
        if frame == 3 {
            shown.textures.insert(5, texture(99));
        }
        let update = tables.update(&objects, &shown, &changed);
        for update in update.into_updates() {
            viewer.apply(update).unwrap();
        }
        assert_eq!(viewer.objects().count(), objects.len());
    }
}

#[path = "scene_tables_tests/lit.rs"]
mod lit;
