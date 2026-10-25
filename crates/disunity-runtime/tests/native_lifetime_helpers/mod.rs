use std::os::unix::fs::MetadataExt;

use serde_json::json;
use worldrunner_disunity_runtime::{CompatibilityHost, HostError, ManagedHostConfig};

mod abi;

fn config() -> ManagedHostConfig {
    crate::support::artifacts().config()
}

pub fn exercise(mode: &str) {
    let mut config = config();
    if mode == "early" {
        let library = abi::initialize_thread(&config.ffi_library);
        config.bridge_type = "Missing.Bridge, Missing.Assembly".into();
        assert!(matches!(
            CompatibilityHost::boot(&config),
            Err(HostError::Hostfxr(_))
        ));
        drop(library);
        return;
    }
    let mut host = CompatibilityHost::boot(&config).unwrap();
    host.create_world().unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 1);
    if mode == "unwind" {
        panic!("intentional unwind with a live native world");
    }
    drop(host);
}

pub fn verify_drained(mode: &str) {
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
    let selected = config();
    let inode = std::fs::metadata(&selected.ffi_library)
        .unwrap()
        .ino()
        .to_string();
    assert!(
        maps.lines()
            .any(|line| line.split_whitespace().nth(4) == Some(&inode)),
        "{mode}: selected native image must stay mapped after worker exit"
    );
    let host = CompatibilityHost::boot(&selected).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert_eq!(roots["data"]["scripts"], 0);
    assert_eq!(roots["data"]["bindings"], 0);
    assert_eq!(roots["data"]["coroutines"], 0);
    assert_eq!(roots["data"]["loaded"], false);
}
