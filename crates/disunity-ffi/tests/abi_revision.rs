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
    let pairs: Vec<(&str, &str)> = sources.iter().map(|source| ("test.rs", *source)).collect();
    abi_revision(build_abi::surface(pairs).as_bytes())
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
    let surface = build_abi::surface([
        ("records.rs", RECORDS),
        ("export.rs", EXPORT),
        ("reasons.rs", REASONS),
    ]);
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

// --- Review item 1: attribute parsing ---------------------------------------------------

#[test]
fn a_doc_comment_mentioning_repr_c_is_not_a_record() {
    let source = r#"
/// See #[repr(C)] for the layout.
///
/// #[repr(C)] is a C attribute.
pub struct DisunityNotARecord {
    pub x: u32,
}
"#;
    assert!(build_abi::records("test.rs", source).is_empty());
}

#[test]
fn a_line_comment_mentioning_repr_c_is_not_a_record() {
    let source = r#"
// #[repr(C)] here would be a record if it were real.
pub struct DisunityNotARecord {
    pub x: u32,
}
"#;
    assert!(build_abi::records("test.rs", source).is_empty());
}

#[test]
fn a_repr_c_align_record_is_in_the_surface_with_its_real_layout() {
    let source = r#"
#[repr(C, align(16))]
pub struct DisunityAligned {
    pub x: u32,
    pub y: u32,
}
"#;
    let records = build_abi::records("test.rs", source);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name, "DisunityAligned");
    assert_eq!(records[0].size, 16);
    assert_eq!(records[0].alignment, 16);
    assert_eq!(records[0].fields[0].offset, 0);
    assert_eq!(records[0].fields[1].offset, 4);
    let line = build_abi::surface([("test.rs", source)]);
    assert!(
        line.contains("record DisunityAligned: size 16, align 16"),
        "{line}"
    );
}

#[test]
fn a_repr_c_packed_record_is_in_the_surface_with_its_real_layout() {
    let source = r#"
#[repr(C, packed)]
pub struct DisunityPacked {
    pub x: u32,
    pub y: u64,
}
"#;
    let records = build_abi::records("test.rs", source);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name, "DisunityPacked");
    assert_eq!(records[0].size, 12);
    assert_eq!(records[0].alignment, 1);
    assert_eq!(records[0].fields[0].offset, 0);
    assert_eq!(records[0].fields[1].offset, 4);
}

#[test]
#[should_panic(expected = "test.rs: DisunityTransparent")]
fn an_unsupported_repr_fails_the_build_naming_file_and_struct() {
    let source = r#"
#[repr(transparent)]
pub struct DisunityTransparent {
    pub x: u32,
}
"#;
    build_abi::records("test.rs", source);
}

#[test]
#[should_panic(expected = "test.rs: DisunityBad")]
fn a_record_with_an_unknown_field_type_fails_naming_file_and_struct() {
    let source = r#"
#[repr(C)]
pub struct DisunityBad {
    pub x: f16,
}
"#;
    build_abi::records("test.rs", source);
}

// --- Review item 2: error messages name the source file -----------------------------------

#[test]
#[should_panic(expected = "test.rs: disunity_no_body")]
fn an_export_with_no_body_fails_naming_file_and_function() {
    let source = r#"
pub extern "C" fn disunity_no_body(world: u64) -> DisunityResult
"#;
    build_abi::exports("test.rs", source);
}

#[test]
#[should_panic(expected = "test.rs: DisunityNameless")]
fn a_record_with_a_malformed_field_fails_naming_file_and_struct() {
    let source = r#"
#[repr(C)]
pub struct DisunityNameless {
    pub x: 1 + 2,
}
"#;
    let _ = build_abi::records("test.rs", source);
}

#[test]
fn a_record_source_runs_from_its_attribute_to_its_closing_brace() {
    let source = "/// Doc.\n#[repr(C)]\n#[derive(Clone, Copy)]\npub struct DisunityPair {\n    pub a: u32,\n    pub b: u32,\n}\n\npub struct Other;\n";
    let records = build_abi::record_sources("test.rs", source);
    assert_eq!(records.len(), 1);
    assert!(records[0].starts_with("#[repr(C)]"));
    assert!(records[0].ends_with("    pub b: u32,\n}"));
}
