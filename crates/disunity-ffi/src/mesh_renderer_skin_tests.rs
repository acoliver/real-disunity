//! The skin a skinned renderer holds: what a write stores, what it refuses and what a later
//! settings write leaves alone. The world holds the skin and no call reads it back, so the tests
//! read it from the session.
use crate::{
    abi::*, mesh_renderer::*, meshes::FIRST_PLAYER_MESH, scheduling::*, store::lock_store,
};
use worldrunner_world_simulation::{SkinBinding, SkinRoot};

fn world() -> u64 {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    world
}

fn new_object(world: u64) -> u64 {
    let mut object = 0;
    assert_eq!(
        unsafe { disunity_gameobject_create(world, &mut object) }.code,
        0
    );
    object
}

/// A renderer of `kind` (0 mesh, 1 skinned) on a new object.
fn renderer_of_kind(world: u64, kind: u32) -> u64 {
    let object = new_object(world);
    let mut renderer = 0;
    let made = unsafe { disunity_mesh_renderer_create(world, object, kind, &mut renderer) };
    assert_eq!(made.code, 0);
    renderer
}

/// The pose a root bone is given: 16 distinct finite values.
fn pose() -> [f32; 16] {
    std::array::from_fn(|i| f32::from(u8::try_from(i).unwrap()) + 0.5)
}

fn write_skin(world: u64, renderer: u64, mesh: u64, bone: u64, pose: Option<&[f32; 16]>) -> i32 {
    let pointer = pose.map_or(std::ptr::null(), |values| values.as_ptr());
    unsafe { disunity_skinned_renderer_write(world, renderer, mesh, bone, pointer) }.code
}

/// What the world holds of the skin of the renderer `handle`: the drawn mesh, the id of the root
/// bone's object and its bind pose.
fn skin_of(world: u64, handle: u64) -> (u64, Option<(u64, [f32; 16])>) {
    let store = lock_store();
    let session = store.session(world).unwrap();
    let (_, renderer) = crate::mesh_renderer::renderer(session, handle).unwrap();
    let SkinBinding { drawn_mesh, root } = renderer.skin;
    let named =
        root.map(|SkinRoot { bone, bind_pose }| (session.objects.id_of(bone).unwrap(), bind_pose));
    (drawn_mesh, named)
}

#[test]
fn a_skinned_renderer_stores_its_drawn_mesh_and_root_bone_with_its_bind_pose() {
    let world = world();
    let renderer = renderer_of_kind(world, 1);
    assert_eq!(skin_of(world, renderer), (0, None));
    let bone = new_object(world);
    let written = write_skin(world, renderer, FIRST_PLAYER_MESH + 3, bone, Some(&pose()));
    assert_eq!(written, 0);
    assert_eq!(
        skin_of(world, renderer),
        (FIRST_PLAYER_MESH + 3, Some((bone, pose())))
    );
    assert_eq!(write_skin(world, renderer, 0, 0, None), 0);
    assert_eq!(skin_of(world, renderer), (0, None));
}

#[test]
fn a_skin_the_world_refuses_changes_nothing() {
    let world = world();
    let renderer = renderer_of_kind(world, 1);
    let bone = new_object(world);
    assert_eq!(
        write_skin(world, renderer, FIRST_PLAYER_MESH, bone, Some(&pose())),
        0
    );
    let before = skin_of(world, renderer);
    let other = self::world();
    let mut foreign = 0;
    for _ in 0..200 {
        foreign = new_object(other);
    }
    let stale = new_object(world);
    assert_eq!(unsafe { disunity_gameobject_destroy(world, stale) }.code, 0);
    let mut advance = DisunityAdvance::default();
    let advanced = unsafe { disunity_world_advance(world, 0, 1_000_000_000, &mut advance) };
    assert_eq!(
        advanced.code, 0,
        "the frame ends and retires the destroyed object"
    );
    let mut not_finite = pose();
    not_finite[5] = f32::NAN;
    let refused = [
        (FIRST_PLAYER_MESH, foreign, Some(pose())),
        (FIRST_PLAYER_MESH, stale, Some(pose())),
        (FIRST_PLAYER_MESH - 1, bone, Some(pose())),
        (FIRST_PLAYER_MESH, bone, Some(not_finite)),
        (FIRST_PLAYER_MESH, bone, None),
        (FIRST_PLAYER_MESH, 0, Some(pose())),
    ];
    for (case, (mesh, root, values)) in refused.iter().enumerate() {
        assert_ne!(
            write_skin(world, renderer, *mesh, *root, values.as_ref()),
            0,
            "refused case {case}"
        );
        assert_eq!(skin_of(world, renderer), before, "case {case}");
    }
}

#[test]
fn only_a_skinned_renderer_has_a_skin() {
    let world = world();
    let mesh_renderer = renderer_of_kind(world, 0);
    assert_ne!(write_skin(world, mesh_renderer, 0, 0, None), 0);
    let mut filter = 0;
    let object = new_object(world);
    assert_eq!(
        unsafe { disunity_mesh_filter_create(world, object, &mut filter) }.code,
        0
    );
    assert_ne!(write_skin(world, filter, 0, 0, None), 0);
}

#[test]
fn writing_a_renderers_settings_keeps_its_skin() {
    let world = world();
    let renderer = renderer_of_kind(world, 1);
    let bone = new_object(world);
    assert_eq!(
        write_skin(world, renderer, FIRST_PLAYER_MESH + 1, bone, Some(&pose())),
        0
    );
    let mut settings = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { disunity_mesh_renderer_read(world, renderer, &mut settings) }.code,
        0
    );
    settings.enabled = 0;
    settings.sorting_order = 4;
    assert_eq!(
        unsafe { disunity_mesh_renderer_write(world, renderer, &settings) }.code,
        0
    );
    assert_eq!(
        skin_of(world, renderer),
        (FIRST_PLAYER_MESH + 1, Some((bone, pose()))),
        "a script that switches the renderer off does not erase what it draws"
    );
    let mut read_back = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { disunity_mesh_renderer_read(world, renderer, &mut read_back) }.code,
        0
    );
    assert_eq!((read_back.enabled, read_back.sorting_order), (0, 4));
}
