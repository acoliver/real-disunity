//! The census of renderers drawn with the white texture and of renderers left out.
use super::*;
use crate::scene_census::{LeftOut, WhiteReason};

/// Content that says a texture failed conversion when its identity is in `failed`.
struct Converting<'a> {
    base: &'a Content,
    failed: BTreeSet<u64>,
}

impl FrameContent for Converting<'_> {
    fn mesh(&self, identity: u64) -> Option<&SceneMesh> {
        self.base.mesh(identity)
    }

    fn texture(&self, identity: u64) -> Option<&SceneTexture> {
        self.base.texture(identity)
    }

    fn missing_texture(&self, identity: u64) -> WhiteReason {
        if self.failed.contains(&identity) {
            WhiteReason::ConversionFailed
        } else {
            WhiteReason::TexelsNotStored
        }
    }
}

fn textured(id: u64, texture: u64) -> MeshObjectView {
    with_slot(object(id, 100, at(0., 0., 0.)), slot([1.; 4], texture))
}

fn without_slot(id: u64) -> MeshObjectView {
    let mut view = object(id, 100, at(0., 0., 0.));
    view.slots.clear();
    view
}

fn counts<T: Ord + Copy>(entries: &[(T, usize)]) -> BTreeMap<T, usize> {
    entries.iter().copied().collect()
}

#[test]
fn each_reason_a_renderer_draws_white_is_counted_once_per_renderer() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let converting = Converting {
        base: &shown,
        failed: [8].into(),
    };
    let objects = [
        without_slot(1),
        textured(2, 0),
        textured(3, 7),
        textured(4, 8),
        textured(5, 7),
    ];
    tables.update(&objects, &converting, &Changes::default());
    let census = tables.census();
    assert_eq!(
        census.last_frame,
        counts(&[
            (WhiteReason::NoMaterialSlot, 1),
            (WhiteReason::TextureZero, 1),
            (WhiteReason::TexelsNotStored, 2),
            (WhiteReason::ConversionFailed, 1),
        ])
    );
    assert_eq!(census.drawn, 5);
    assert!(census.left_out.is_empty());
}

#[test]
fn a_texture_table_without_a_free_handle_still_draws_white_and_is_counted() {
    let mut tables = SceneTables::new().unwrap();
    tables.textures = u32::MAX;
    let mut shown = content(&[100]);
    shown.textures.insert(7, texture(90));
    let first = update(&mut tables, &[textured(1, 7)], &shown);
    assert_eq!(
        material_of(&first, adds(&first)[0]),
        0,
        "the white material"
    );
    assert_eq!(appended_textures(&first), 0);
    assert_eq!(
        tables.census().last_frame,
        counts(&[(WhiteReason::HandleOverflow, 1)])
    );
}

#[test]
fn renderers_with_a_texture_are_counted_only_in_the_denominator() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    shown.textures.insert(7, texture(90));
    update(&mut tables, &[textured(1, 7), textured(2, 7)], &shown);
    let census = tables.census();
    assert_eq!(census.drawn, 2);
    assert!(census.last_frame.is_empty());
    assert!(census.most_in_one_frame.is_empty());
}

#[test]
fn texels_arriving_clear_the_last_frame_and_keep_the_worst_frame() {
    let mut tables = SceneTables::new().unwrap();
    let mut shown = content(&[100]);
    let objects = [textured(1, 7), textured(2, 7), textured(3, 0)];
    update(&mut tables, &objects, &shown);
    shown.textures.insert(7, texture(90));
    let changed = Changes {
        meshes: BTreeSet::new(),
        textures: [7].into(),
    };
    tables.update(&objects, &shown, &changed);
    let census = tables.census();
    assert_eq!(
        census.last_frame,
        counts(&[(WhiteReason::TextureZero, 1)]),
        "the textured renderers are no longer white"
    );
    assert_eq!(
        census.most_in_one_frame,
        counts(&[
            (WhiteReason::TexelsNotStored, 2),
            (WhiteReason::TextureZero, 1)
        ])
    );
    assert_eq!(census.drawn, 3);
}

#[test]
fn the_worst_frame_of_a_reason_is_its_largest_count_in_any_frame() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    update(
        &mut tables,
        &[textured(1, 0), textured(2, 0), textured(3, 0)],
        &shown,
    );
    update(&mut tables, &[textured(1, 0)], &shown);
    let census = tables.census();
    assert_eq!(census.last_frame, counts(&[(WhiteReason::TextureZero, 1)]));
    assert_eq!(
        census.most_in_one_frame,
        counts(&[(WhiteReason::TextureZero, 3)])
    );
}

#[test]
fn objects_that_do_not_change_count_the_same_in_every_frame() {
    let mut tables = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let objects = [textured(1, 0), textured(2, 7)];
    update(&mut tables, &objects, &shown);
    let first = tables.census().clone();
    assert_eq!(
        first.last_frame,
        counts(&[
            (WhiteReason::TextureZero, 1),
            (WhiteReason::TexelsNotStored, 1)
        ])
    );
    assert_eq!(first.drawn, 2);
    for _ in 0..3 {
        let unchanged = update(&mut tables, &objects, &shown);
        assert!(unchanged.edits.is_empty());
        assert_eq!(
            tables.census(),
            &first,
            "the same census in a frame that changes nothing"
        );
    }
}

#[test]
fn counting_changes_no_handle_material_or_edit() {
    let mut counted = SceneTables::new().unwrap();
    let shown = content(&[100]);
    let objects = [textured(1, 0), textured(2, 7), without_slot(3)];
    let first = update(&mut counted, &objects, &shown);
    assert_eq!(adds(&first).len(), 3);
    assert!(
        adds(&first)
            .into_iter()
            .all(|id| material_of(&first, id) == 0),
        "every renderer draws with the white material"
    );
    assert_eq!(appended_textures(&first), 0);
    assert_eq!(appended_materials(&first), 0);
    assert!(update(&mut counted, &objects, &shown).edits.is_empty());
}

#[test]
fn a_renderer_whose_mesh_has_not_arrived_is_left_out_and_not_counted_as_white() {
    let mut tables = SceneTables::new().unwrap();
    let objects = [
        object(7, 100, at(0., 0., 0.)),
        object(8, 101, at(1., 0., 0.)),
    ];
    update(&mut tables, &objects, &content(&[100]));
    let census = tables.census();
    assert_eq!(census.left_out, counts(&[(LeftOut::MeshNotArrived, 1)]));
    assert_eq!(census.drawn, 1);
    assert_eq!(census.last_frame, counts(&[(WhiteReason::TextureZero, 1)]));
    update(&mut tables, &objects, &content(&[100, 101]));
    assert!(tables.census().left_out.is_empty());
    assert_eq!(tables.census().drawn, 2);
}

#[test]
fn a_mesh_table_without_a_free_handle_leaves_the_renderer_out() {
    let mut tables = SceneTables::new().unwrap();
    tables.meshes = u32::MAX;
    let first = update(
        &mut tables,
        &[object(7, 100, at(0., 0., 0.))],
        &content(&[100]),
    );
    assert!(adds(&first).is_empty());
    let census = tables.census();
    assert_eq!(census.left_out, counts(&[(LeftOut::MeshHandleOverflow, 1)]));
    assert_eq!(census.drawn, 0);
    assert!(census.last_frame.is_empty());
}

#[test]
fn a_renderer_the_scene_cannot_place_is_neither_drawn_nor_left_out() {
    let mut tables = SceneTables::new().unwrap();
    let mut bad = textured(1, 0);
    bad.local_to_world[4] = 0.5;
    let first = update(&mut tables, &[bad, textured(2, 0)], &content(&[100]));
    assert_eq!(first.skipped.len(), 1);
    let census = tables.census();
    assert_eq!(census.drawn, 1);
    assert_eq!(census.last_frame, counts(&[(WhiteReason::TextureZero, 1)]));
    assert!(census.left_out.is_empty());
}
