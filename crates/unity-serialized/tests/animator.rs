#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    animator::{AnimationClip, Animator, AnimatorOverrideController, ClipOverride},
    animator_controller::AnimatorController,
    object::PPtr,
    write::ObjectWriter,
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

/// An animator laid out like Old World's: 56 bytes, a game object, the enabled byte padded to
/// four, an avatar, a controller, culling and update modes, then two groups of flag bytes.
fn animator_bytes(big_endian: bool, flags: [u8; 8]) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(0, 17341)).u8(1).align();
    w.pptr(ptr(0, 9085)).pptr(ptr(0, 13848));
    w.i32(2).i32(1);
    w.bytes(&flags);
    w.finish()
}

#[test]
fn decodes_an_animator_laid_out_like_old_worlds() {
    let bytes = animator_bytes(false, [1, 0, 0, 0, 1, 1, 0, 0]);
    assert_eq!(bytes.len(), 56);
    let animator = decode_one::<Animator>(ClassId::ANIMATOR, bytes, false).unwrap();
    assert_eq!(animator.game_object, ptr(0, 17341));
    assert!(animator.enabled);
    assert_eq!(animator.avatar, ptr(0, 9085));
    assert_eq!(animator.controller, ptr(0, 13848));
    assert_eq!((animator.culling_mode, animator.update_mode), (2, 1));
    assert!(animator.motion.apply_root_motion);
    assert!(!animator.motion.linear_velocity_blending && !animator.motion.stabilize_feet);
    assert!(animator.hierarchy.has_transform_hierarchy);
    assert!(animator.hierarchy.allow_constant_clip_sampling);
    assert!(!animator.on_disable.keep_state && !animator.on_disable.write_defaults);
}

#[test]
fn decodes_a_big_endian_animator() {
    let bytes = animator_bytes(true, [0, 0, 0, 0, 0, 1, 0, 0]);
    let animator = decode_one::<Animator>(ClassId::ANIMATOR, bytes, true).unwrap();
    assert_eq!(animator.controller, ptr(0, 13848));
    assert_eq!((animator.culling_mode, animator.update_mode), (2, 1));
    assert!(!animator.hierarchy.has_transform_hierarchy);
    assert!(animator.hierarchy.allow_constant_clip_sampling);
}

#[test]
fn each_flag_byte_reads_into_its_own_field() {
    let read = |flags: [u8; 8]| {
        decode_one::<Animator>(ClassId::ANIMATOR, animator_bytes(false, flags), false).unwrap()
    };
    assert!(
        read([0, 1, 0, 0, 0, 0, 0, 0])
            .motion
            .linear_velocity_blending
    );
    assert!(read([0, 0, 1, 0, 0, 0, 0, 0]).motion.stabilize_feet);
    assert!(
        read([0, 0, 0, 0, 1, 0, 0, 0])
            .hierarchy
            .has_transform_hierarchy
    );
    assert!(
        read([0, 0, 0, 0, 0, 1, 0, 0])
            .hierarchy
            .allow_constant_clip_sampling
    );
    assert!(read([0, 0, 0, 0, 0, 0, 1, 0]).on_disable.keep_state);
    assert!(read([0, 0, 0, 0, 0, 0, 0, 1]).on_disable.write_defaults);
    assert!(!read([0; 8]).motion.apply_root_motion);
}

#[test]
fn a_disabled_animator_with_no_controller_decodes() {
    let mut bytes = animator_bytes(false, [0; 8]);
    bytes[12] = 0;
    bytes[28..40].fill(0);
    let animator = decode_one::<Animator>(ClassId::ANIMATOR, bytes, false).unwrap();
    assert!(!animator.enabled);
    assert!(animator.controller.is_null());
}

fn assert_every_prefix_fails<T: worldrunner_unity_serialized::object::Layout>(
    class: ClassId,
    bytes: &[u8],
) {
    for end in 0..bytes.len() {
        let error = decode_one::<T>(class, bytes[..end].to_vec(), false)
            .err()
            .unwrap_or_else(|| panic!("prefix {end} decoded"));
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
}

fn assert_trailing_byte_fails<T: worldrunner_unity_serialized::object::Layout>(
    class: ClassId,
    mut bytes: Vec<u8>,
) {
    bytes.push(0);
    let error = decode_one::<T>(class, bytes, false)
        .err()
        .expect("trailing");
    assert!(
        matches!(error, SerializedError::TrailingBytes { .. }),
        "{error}"
    );
}

#[test]
fn an_animator_must_be_complete_and_end_where_the_layout_ends() {
    let bytes = animator_bytes(false, [0; 8]);
    assert_every_prefix_fails::<Animator>(ClassId::ANIMATOR, &bytes);
    assert_trailing_byte_fails::<Animator>(ClassId::ANIMATOR, bytes);
}

/// "Nets Animator" of Old World: a base controller and three clip pairs.
fn override_bytes(big_endian: bool, pairs: &[(PPtr, PPtr)]) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Nets Animator").unwrap().align();
    w.pptr(ptr(0, 9186));
    w.i32(i32::try_from(pairs.len()).unwrap());
    for &(original, replacement) in pairs {
        w.pptr(original).pptr(replacement);
    }
    w.finish()
}

fn three_pairs() -> Vec<(PPtr, PPtr)> {
    vec![
        (ptr(0, 100), ptr(0, 200)),
        (ptr(0, 101), ptr(0, 0)),
        (ptr(1, 102), ptr(1, 202)),
    ]
}

#[test]
fn decodes_an_override_controller_with_its_clip_pairs() {
    let bytes = override_bytes(false, &three_pairs());
    let decoded = decode_one::<AnimatorOverrideController>(
        ClassId::ANIMATOR_OVERRIDE_CONTROLLER,
        bytes,
        false,
    )
    .unwrap();
    assert_eq!(decoded.name, "Nets Animator");
    assert_eq!(decoded.base_controller, ptr(0, 9186));
    assert_eq!(
        decoded.overrides,
        vec![
            ClipOverride {
                original: ptr(0, 100),
                replacement: ptr(0, 200)
            },
            ClipOverride {
                original: ptr(0, 101),
                replacement: ptr(0, 0)
            },
            ClipOverride {
                original: ptr(1, 102),
                replacement: ptr(1, 202)
            },
        ]
    );
}

#[test]
fn decodes_a_big_endian_override_controller_without_pairs() {
    let bytes = override_bytes(true, &[]);
    let decoded = decode_one::<AnimatorOverrideController>(
        ClassId::ANIMATOR_OVERRIDE_CONTROLLER,
        bytes,
        true,
    )
    .unwrap();
    assert_eq!(decoded.base_controller, ptr(0, 9186));
    assert!(decoded.overrides.is_empty());
}

#[test]
fn an_override_controller_must_be_complete_and_end_where_the_layout_ends() {
    let bytes = override_bytes(false, &three_pairs());
    assert_every_prefix_fails::<AnimatorOverrideController>(
        ClassId::ANIMATOR_OVERRIDE_CONTROLLER,
        &bytes,
    );
    assert_trailing_byte_fails::<AnimatorOverrideController>(
        ClassId::ANIMATOR_OVERRIDE_CONTROLLER,
        bytes,
    );
}

#[test]
fn an_animation_clip_reads_its_name_and_takes_the_rest_of_the_object() {
    let mut w = ObjectWriter::new(false);
    w.string("Harbor_Idle_A").unwrap().align();
    w.bytes(&[9; 40]);
    let clip = decode_one::<AnimationClip>(ClassId::ANIMATION_CLIP, w.finish(), false).unwrap();
    assert_eq!(clip.name, "Harbor_Idle_A");
}

/// What a controller holds that the decoder reads: its clips.
struct Parts {
    layers: u32,
    clips: Vec<PPtr>,
    reserved_count: i32,
}

fn node(w: &mut ObjectWriter) {
    w.i32(1).i32(-1).i32(-1);
    w.i32(2).u32(1).u32(2);
    w.i32(2).f32(0.0).f32(1.0);
    w.i32(1).floats(&[0.5, 0.5]);
    w.i32(1).f32(0.7);
    w.i32(1).floats(&[0.1, 0.2]);
    w.i32(1).f32(1.4);
    w.i32(1).i32(2).u32(0).u32(1);
    w.i32(1).u32(5).u8(1).align();
    w.u32(3).f32(1.0).f32(0.0).u8(0).align();
}

fn transition(w: &mut ObjectWriter) {
    w.i32(2);
    for mode in [1u32, 4] {
        w.u32(mode).u32(0).f32(0.5).f32(0.0);
    }
    w.u32(7).u32(8).u32(9).u32(10);
    w.f32(0.25).f32(0.0).f32(0.75);
    w.bytes(&[1, 0, 0, 0]).i32(0).bytes(&[0, 1, 0, 0]);
}

fn state(w: &mut ObjectWriter) {
    w.i32(1);
    transition(w);
    w.i32(1).i32(0);
    w.i32(1).i32(1);
    node(w);
    for id in 100..108 {
        w.u32(id);
    }
    w.f32(1.0).f32(0.0).bytes(&[0, 1, 1, 0]);
}

fn machine(w: &mut ObjectWriter) {
    w.i32(1);
    state(w);
    w.i32(1);
    transition(w);
    w.i32(1);
    w.i32(1).u32(0).i32(1);
    w.u32(1).u32(2).u32(3).u32(4);
    w.u32(5).bytes(&[1, 0, 0, 0]);
    w.u32(0).u32(1);
}

fn controller_bytes(big_endian: bool, parts: &Parts) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Harbor Animator Controller").unwrap().align();
    w.u32(912);
    w.u32(parts.layers);
    for _ in 0..parts.layers {
        w.u32(0).u32(0).u32(u32::MAX).u32(u32::MAX).u32(0x7ffff);
        w.i32(2).u32(11).f32(1.0).u32(12).f32(0.5);
        w.u32(756_556_552).i32(0).f32(0.0).bytes(&[0, 0, 0, 0]);
    }
    w.i32(1);
    machine(&mut w);
    w.i32(1).u32(1_414_469_382).u32(9).u32(0);
    w.i32(1).floats(&[1.0, 2.0, 3.0]);
    w.i32(0).i32(0);
    w.i32(1).f32(0.5);
    w.i32(0);
    w.i32(3).bytes(&[1, 0, 1]).align();
    w.i32(parts.reserved_count);
    w.i32(2);
    w.u32(2_105_523_844)
        .string("GravityWeight")
        .unwrap()
        .align();
    w.u32(756_556_552).string("Base Layer").unwrap().align();
    w.i32(i32::try_from(parts.clips.len()).unwrap());
    for &clip in &parts.clips {
        w.pptr(clip);
    }
    w.i32(1).u32(756_556_552).i32(0).u32(0).u32(2);
    w.i32(2).u32(0).u32(1);
    w.i32(1).pptr(ptr(0, 44_043));
    w.u8(0).align();
    w.finish()
}

fn parts() -> Parts {
    Parts {
        layers: 2,
        clips: vec![ptr(0, 8488), ptr(0, 8489), ptr(1, 12)],
        reserved_count: 0,
    }
}

#[test]
fn decodes_the_name_and_clips_of_a_controller_with_layers_states_and_blend_trees() {
    let bytes = controller_bytes(false, &parts());
    let decoded =
        decode_one::<AnimatorController>(ClassId::ANIMATOR_CONTROLLER, bytes, false).unwrap();
    assert_eq!(decoded.name, "Harbor Animator Controller");
    assert_eq!(decoded.clips, parts().clips);
}

#[test]
fn decodes_a_big_endian_controller_with_no_clips() {
    let mut parts = parts();
    parts.clips.clear();
    let bytes = controller_bytes(true, &parts);
    let decoded =
        decode_one::<AnimatorController>(ClassId::ANIMATOR_CONTROLLER, bytes, true).unwrap();
    assert!(decoded.clips.is_empty());
}

#[test]
fn a_controller_without_layers_decodes() {
    let mut parts = parts();
    parts.layers = 0;
    let bytes = controller_bytes(false, &parts);
    let decoded =
        decode_one::<AnimatorController>(ClassId::ANIMATOR_CONTROLLER, bytes, false).unwrap();
    assert_eq!(decoded.clips.len(), 3);
}

#[test]
fn a_controller_must_be_complete_and_end_where_the_layout_ends() {
    let bytes = controller_bytes(false, &parts());
    assert_every_prefix_fails::<AnimatorController>(ClassId::ANIMATOR_CONTROLLER, &bytes);
    assert_trailing_byte_fails::<AnimatorController>(ClassId::ANIMATOR_CONTROLLER, bytes);
}

#[test]
fn a_controller_whose_reserved_count_is_not_zero_is_not_read_as_if_it_were() {
    let mut parts = parts();
    parts.reserved_count = 1;
    let bytes = controller_bytes(false, &parts);
    let error = decode_one::<AnimatorController>(ClassId::ANIMATOR_CONTROLLER, bytes, false)
        .expect_err("a count the decoder does not know the entries of");
    assert!(
        matches!(error, SerializedError::InvalidCount { .. }),
        "{error}"
    );
}
