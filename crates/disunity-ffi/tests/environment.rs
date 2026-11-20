//! The environment lighting a world holds, through the ABI: a read of a world nobody wrote gives
//! the values of a new scene with both sections unset, a write is read back exactly, and a bad
//! record is refused and changes nothing.
use worldrunner_disunity_ffi::{abi::*, environment::*};

const FIRST_TEXTURE: u64 = 1 << 40;

fn world() -> u64 {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    world
}

fn read(world: u64) -> DisunityEnvironment {
    let mut record = DisunityEnvironment::default_for_test();
    assert_eq!(
        unsafe { disunity_environment_read(world, &mut record) }.code,
        0
    );
    record
}

fn write(world: u64, record: &DisunityEnvironment) -> i32 {
    unsafe { disunity_environment_write(world, record) }.code
}

trait Fresh {
    fn default_for_test() -> Self;
}

impl Fresh for DisunityEnvironment {
    fn default_for_test() -> Self {
        // Every field away from what a read writes, so a read that skips one is seen.
        Self {
            ambient_set: 9,
            ambient_mode: 9,
            ambient_intensity: -1.0,
            reflection_set: 9,
            reflection_mode: 9,
            reflection_intensity: -1.0,
            custom_kind: 9,
            skybox_kind: 9,
            custom_texture: 9,
            skybox_texture: 9,
            ambient_sky: [-1.0; 4],
            ambient_equator: [-1.0; 4],
            ambient_ground: [-1.0; 4],
            ambient_flat: [-1.0; 4],
            ambient_probe: [-1.0; 27],
            padding: 9,
        }
    }
}

fn lit() -> DisunityEnvironment {
    DisunityEnvironment {
        ambient_set: 1,
        ambient_mode: 4,
        ambient_intensity: 1.5,
        reflection_set: 1,
        reflection_mode: 1,
        reflection_intensity: 0.5,
        custom_kind: 1,
        skybox_kind: 1,
        custom_texture: FIRST_TEXTURE + 2,
        skybox_texture: FIRST_TEXTURE + 3,
        ambient_sky: [0.1, 0.2, 0.3, 1.0],
        ambient_equator: [0.4, 0.5, 0.6, 1.0],
        ambient_ground: [0.7, 0.8, 0.9, 1.0],
        ambient_flat: [0.3, 0.2, 0.1, 1.0],
        ambient_probe: std::array::from_fn(|index| 0.25 * index as f32 - 3.0),
        padding: 0,
    }
}

#[test]
fn a_world_nobody_wrote_reads_both_sections_unset_with_the_values_of_a_new_scene() {
    let world = world();
    let record = read(world);
    assert_eq!((record.ambient_set, record.reflection_set), (0, 0));
    assert_eq!((record.ambient_mode, record.reflection_mode), (0, 0));
    assert_eq!(
        (record.ambient_intensity, record.reflection_intensity),
        (1.0, 1.0)
    );
    assert_eq!((record.custom_kind, record.custom_texture), (0, 0));
    assert_eq!((record.skybox_kind, record.skybox_texture), (0, 0));
    assert_eq!(record.ambient_probe, [0.0; 27]);
    assert_eq!(record.ambient_sky, record.ambient_flat);
    assert_eq!(record.padding, 0);
}

#[test]
fn a_written_environment_reads_back_exactly() {
    let world = world();
    assert_eq!(write(world, &lit()), 0);
    assert_eq!(read(world), lit());
}

#[test]
fn a_section_written_unset_goes_back_to_unwritten_whatever_else_the_record_holds() {
    let world = world();
    assert_eq!(write(world, &lit()), 0);
    let reflection_only = DisunityEnvironment {
        ambient_set: 0,
        ..lit()
    };
    assert_eq!(write(world, &reflection_only), 0);
    let record = read(world);
    assert_eq!((record.ambient_set, record.reflection_set), (0, 1));
    assert_eq!(record.ambient_mode, 0);
    assert_eq!(record.ambient_probe, [0.0; 27]);
    assert_eq!(record.custom_texture, FIRST_TEXTURE + 2);
}

#[test]
fn a_bad_record_is_refused_and_the_environment_stays_as_it_was() {
    let world = world();
    assert_eq!(write(world, &lit()), 0);
    let mut not_finite = lit();
    not_finite.ambient_probe[3] = f32::NAN;
    let mut negative = lit();
    negative.reflection_intensity = -0.1;
    let mut ambient_mode = lit();
    ambient_mode.ambient_mode = 2;
    let mut reflection_mode = lit();
    reflection_mode.reflection_mode = 2;
    let mut flag = lit();
    flag.reflection_set = 2;
    let mut padding = lit();
    padding.padding = 1;
    let mut small_identity = lit();
    small_identity.custom_texture = 5;
    let mut missing_render_texture = lit();
    missing_render_texture.skybox_kind = 2;
    missing_render_texture.skybox_texture = 77;
    let mut temporary = lit();
    temporary.custom_kind = 4;
    temporary.custom_texture = 3;
    for refused in [
        not_finite,
        negative,
        ambient_mode,
        reflection_mode,
        flag,
        padding,
        small_identity,
        missing_render_texture,
        temporary,
    ] {
        assert_ne!(write(world, &refused), 0, "{refused:?}");
    }
    assert_eq!(read(world), lit());
}

#[test]
fn a_null_pointer_and_a_world_that_does_not_exist_are_refused() {
    let world = world();
    assert_ne!(
        unsafe { disunity_environment_read(world, std::ptr::null_mut()) }.code,
        0
    );
    assert_ne!(
        unsafe { disunity_environment_write(world, std::ptr::null()) }.code,
        0
    );
    let mut record = DisunityEnvironment::default_for_test();
    assert_ne!(
        unsafe { disunity_environment_read(world + 1000, &mut record) }.code,
        0
    );
    assert_ne!(write(world + 1000, &lit()), 0);
}
