use super::extraction::{image_size, sprite_key};
use serde_json::json;

#[test]
fn texture_admission_bounds_dimensions_before_allocating_decoded_pixels() {
    assert_eq!(image_size([64, 32]).unwrap(), 8192);
    assert!(image_size([0, 32]).is_err());
    assert!(image_size([4097, 1]).is_err());
    assert!(image_size([u32::MAX, u32::MAX]).is_err());
}

#[test]
fn sprite_identity_keeps_subasset_file_id_and_explicit_visual_replacement() {
    assert_eq!(
        sprite_key(&json!({"guid":"sprite-guid","file_id":-12})).as_deref(),
        Some("sprite-guid:-12")
    );
    assert_eq!(
        sprite_key(&json!({"$builtin":"authored-square"})).as_deref(),
        Some("authored-square")
    );
    assert!(sprite_key(&json!({"$missing":{"guid":"absent","file_id":1}})).is_none());
}

#[test]
fn extraction_profile_is_opt_in_and_appends_completed_stage_costs() {
    use std::time::Duration;
    let root = tempfile::tempdir().unwrap();
    let times = [12, 14, 15, 16].map(Duration::from_millis);
    let write = |quads, enabled| {
        crate::playback::profile_extraction(root.path(), times, quads, enabled).unwrap();
    };
    write(100, false);
    let output = root.path().join("extraction-profile.ndjson");
    assert!(!output.exists());
    write(100, true);
    write(99, true);
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        json!({"snapshot_ms":12.0,"layout_ms":2.0,
        "extract_ms":1.0,"glyphs_ms":1.0,"quads":100})
    );
    assert_eq!(rows[1]["quads"], 99);
}

#[test]
fn disabled_extraction_profile_never_opens_an_output_path() {
    let absent = tempfile::tempdir().unwrap().path().join("absent");
    let times = [std::time::Duration::ZERO; 4];
    assert!(crate::playback::profile_extraction(&absent, times, 0, false).is_ok());
    assert!(crate::playback::profile_extraction(&absent, times, 0, true).is_err());
}
