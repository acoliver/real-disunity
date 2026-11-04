use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use worldrunner_save::{
    ApplicationStore, SaveError, SettingValue, SlotId, SlotMetadata, StoreLimits,
};

fn directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/save-tests")
        .join(format!("{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn limits() -> StoreLimits {
    StoreLimits {
        payload_bytes: 1024,
        label_bytes: 32,
        settings_bytes: 256,
        value_bytes: 32,
        settings_entries: 8,
    }
}

#[test]
fn slot_names_cannot_escape_or_alias() {
    for name in [
        "",
        ".",
        "..",
        "a..b",
        "../a",
        "/root",
        "a/b",
        "a\\b",
        "C:\\a",
        "A",
        "a.b",
        "a b",
        "é",
        "a\0b",
        &"a".repeat(65),
    ] {
        assert!(SlotId::new(name).is_err(), "{name:?}");
    }
    assert_eq!(
        SlotId::new("quick-save_01").unwrap().as_str(),
        "quick-save_01"
    );
}

#[test]
fn list_is_sorted_and_metadata_and_bytes_round_trip() {
    let root = directory("slots");
    let store = ApplicationStore::new(root.clone(), limits()).unwrap();
    let metadata = SlotMetadata {
        label: "First save".into(),
        payload_version: 7,
    };
    for name in ["z", "a"] {
        store
            .write_slot(&SlotId::new(name).unwrap(), b"payload", &metadata)
            .unwrap();
    }
    let listed = store.list_slots().unwrap();
    assert_eq!(
        listed.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(listed[0].metadata, metadata);
    assert_eq!(listed[0].size, 7);
    assert!(listed[0].modified <= SystemTime::now());
    let id = SlotId::new("a").unwrap();
    assert_eq!(store.read_slot(&id).unwrap().payload, b"payload");
    store.delete_slot(&id).unwrap();
    assert_eq!(store.list_slots().unwrap().len(), 1);
    assert!(
        matches!(store.read_slot(&id), Err(SaveError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_round_trip_and_reject_oversize_values() {
    let root = directory("settings");
    let store = ApplicationStore::new(root.clone(), limits()).unwrap();
    let mut settings = BTreeMap::from([
        ("bool".into(), SettingValue::Bool(true)),
        ("integer".into(), SettingValue::Integer(-42)),
        ("float".into(), SettingValue::Float(1.25)),
        ("string".into(), SettingValue::String("Ada".into())),
    ]);
    store.write_settings(&settings).unwrap();
    assert_eq!(store.read_settings().unwrap(), settings);
    settings.insert("large".into(), SettingValue::String("x".repeat(33)));
    assert!(matches!(
        store.write_settings(&settings),
        Err(SaveError::PayloadTooLarge { .. })
    ));
    assert_eq!(store.read_settings().unwrap().len(), 4);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_and_future_envelopes_fail_without_reading_large_bodies() {
    let root = directory("bounded");
    let store = ApplicationStore::new(root.clone(), limits()).unwrap();
    let id = SlotId::new("a").unwrap();
    let metadata = SlotMetadata {
        label: String::new(),
        payload_version: 1,
    };
    store.write_slot(&id, b"ok", &metadata).unwrap();
    let path = root.join("slot-a.rds");
    let original = fs::read(&path).unwrap();
    let mut bytes = original.clone();
    bytes[8..12].copy_from_slice(&99_u32.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(1 << 32)
        .unwrap();
    assert!(matches!(
        store.read_slot(&id),
        Err(SaveError::UnsupportedVersion(99))
    ));
    fs::write(&path, &original).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(1 << 32)
        .unwrap();
    assert!(matches!(
        store.read_slot(&id),
        Err(SaveError::PayloadTooLarge { .. })
    ));
    fs::write(&path, &original[..20]).unwrap();
    assert!(matches!(store.read_slot(&id), Err(SaveError::Truncated)));
    let mut corrupt = original;
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(&path, corrupt).unwrap();
    assert!(matches!(
        store.read_slot(&id),
        Err(SaveError::PayloadDigestMismatch)
    ));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn read_only_directory_reports_permission_error() {
    use std::os::unix::fs::PermissionsExt;
    let root = directory("permission");
    let store = ApplicationStore::new(root.clone(), limits()).unwrap();
    let original = fs::metadata(&root).unwrap().permissions();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.write_settings(&BTreeMap::new());
    fs::set_permissions(&root, original).unwrap();
    assert!(
        matches!(result, Err(SaveError::Io(e)) if e.kind() == std::io::ErrorKind::PermissionDenied)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_limits_reject_oversize_payload_metadata_and_settings_documents() {
    let root = directory("limits");
    let store = ApplicationStore::new(root.clone(), limits()).unwrap();
    let id = SlotId::new("a").unwrap();
    let metadata = SlotMetadata {
        label: "x".repeat(33),
        payload_version: 0,
    };
    assert!(matches!(
        store.write_slot(&id, b"", &metadata),
        Err(SaveError::PayloadTooLarge { .. })
    ));
    let metadata = SlotMetadata {
        label: String::new(),
        payload_version: 0,
    };
    assert!(matches!(
        store.write_slot(&id, &[0; 1025], &metadata),
        Err(SaveError::PayloadTooLarge { .. })
    ));
    let entries: BTreeMap<_, _> = (0..9)
        .map(|i| (i.to_string(), SettingValue::Bool(false)))
        .collect();
    assert!(matches!(
        store.write_settings(&entries),
        Err(SaveError::PayloadTooLarge { .. })
    ));
    let entries: BTreeMap<_, _> = (0..8)
        .map(|i| (i.to_string(), SettingValue::String("x".repeat(32))))
        .collect();
    assert!(matches!(
        store.write_settings(&entries),
        Err(SaveError::PayloadTooLarge { .. })
    ));
    let mut invalid = limits();
    invalid.payload_bytes = 0;
    assert!(matches!(
        ApplicationStore::new(root.clone(), invalid),
        Err(SaveError::InvalidLimits)
    ));
    fs::remove_dir_all(root).unwrap();
}
