//! The ABI revision is derived from the exported surface: function signatures, `#[repr(C)]`
//! record layouts and error reasons. Prose never moves it, and no change to the surface can
//! leave it where it was.

use worldrunner_disunity_ffi::abi_revision;

#[path = "../build_abi.rs"]
mod build_abi;

const RECORDS: &str = r#"
/// A point in the world.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityPoint {
    pub x: f32,
    pub y: f32,
}
"#;

const EXPORT: &str = r#"
/// Moves a point.
///
/// # Safety
/// `point` must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_point_move(world: u64, point: *mut DisunityPoint) -> DisunityResult {
    unsafe { *point = DisunityPoint { x: 1.0, y: 2.0 } };
    DisunityResult::ok()
}
"#;

const REASONS: &str = r#"
#[repr(u64)]
pub enum DisunityErrorReason {
    InvalidArgument = 1,
    Faulted = 5,
}
"#;

fn revision(sources: &[&str]) -> u32 {
    abi_revision(build_abi::surface(sources.iter().copied()).as_bytes())
}

fn baseline() -> u32 {
    revision(&[RECORDS, EXPORT, REASONS])
}

#[test]
fn the_revision_is_the_fnv_1a_hash_of_the_surface_text() {
    assert_eq!(abi_revision(b""), 0x811c_9dc5);
    assert_eq!(abi_revision(b"a"), 0xe40c_292c);
    assert_eq!(abi_revision(b"foobar"), 0xbf9c_f968);
}

#[test]
fn the_surface_lists_the_exports_records_and_reasons_of_the_sources() {
    let surface = build_abi::surface([RECORDS, EXPORT, REASONS]);
    assert!(
        surface.contains(
            "fn disunity_point_move(world: u64, point: *mut DisunityPoint) -> DisunityResult"
        ),
        "{surface}"
    );
    assert!(
        surface.contains("record DisunityPoint: size 8, align 4 { x: f32 @0, y: f32 @4 }"),
        "{surface}"
    );
    assert!(surface.contains("reason Faulted = 5"), "{surface}");
}

#[test]
fn prose_edits_leave_the_revision_unchanged() {
    let edited_records = r#"
/// A point, reworded and expanded.
// A plain comment.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityPoint {
    /// East.
    pub x: f32,

    // North.
    pub y: f32,
}
"#;
    let edited_export = r#"
/// Moves a point somewhere else, with different words.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_point_move(
    world: u64, // the world
    point: *mut DisunityPoint,
) -> DisunityResult {
    // a different body
    DisunityResult::ok()
}
"#;
    let edited_reasons = r#"
/// Why a call failed.
#[repr(u64)]
pub enum DisunityErrorReason {
    /// Bad input.
    InvalidArgument = 1,
    // Gone wrong.
    Faulted = 5,
}
"#;
    assert_eq!(
        revision(&[edited_records, edited_export, edited_reasons]),
        baseline()
    );
}

#[test]
fn moving_an_export_between_source_files_leaves_the_revision_unchanged() {
    assert_eq!(revision(&[EXPORT, REASONS, RECORDS]), baseline());
    assert_eq!(
        revision(&[&format!("{RECORDS}{EXPORT}"), REASONS]),
        baseline()
    );
}

#[test]
fn a_new_record_field_changes_the_revision() {
    let wider = RECORDS.replace("pub y: f32,", "pub y: f32,\n    pub z: f32,");
    assert_ne!(revision(&[&wider, EXPORT, REASONS]), baseline());
}

#[test]
fn a_field_of_another_type_and_the_same_size_changes_the_revision() {
    let retyped = RECORDS.replace("pub y: f32,", "pub y: u32,");
    assert_ne!(revision(&[&retyped, EXPORT, REASONS]), baseline());
}

#[test]
fn a_changed_argument_list_changes_the_revision() {
    for changed in [
        EXPORT.replace("world: u64, ", ""),
        EXPORT.replace("world: u64", "world: u32"),
        EXPORT.replace("point: *mut", "point: *const"),
        EXPORT.replace("world: u64,", "world: u64, flags: u32,"),
        EXPORT.replace("-> DisunityResult", "-> u32"),
        EXPORT.replace("disunity_point_move", "disunity_point_shift"),
    ] {
        assert_ne!(
            revision(&[RECORDS, &changed, REASONS]),
            baseline(),
            "{changed}"
        );
    }
}

#[test]
fn adding_or_removing_an_export_changes_the_revision() {
    let second = EXPORT.replace("disunity_point_move", "disunity_point_reset");
    assert_ne!(revision(&[RECORDS, EXPORT, &second, REASONS]), baseline());
    assert_ne!(revision(&[RECORDS, REASONS]), baseline());
}

#[test]
fn a_new_or_renumbered_error_reason_changes_the_revision() {
    let added = REASONS.replace("Faulted = 5,", "Faulted = 5,\n    Ui = 6,");
    let renumbered = REASONS.replace("Faulted = 5", "Faulted = 6");
    assert_ne!(revision(&[RECORDS, EXPORT, &added]), baseline());
    assert_ne!(revision(&[RECORDS, EXPORT, &renumbered]), baseline());
}
