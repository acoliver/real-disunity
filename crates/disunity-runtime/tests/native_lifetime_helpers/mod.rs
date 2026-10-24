use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::sync::OnceLock;

use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
use serde_json::json;
use sha2::{Digest, Sha256};
use worldrunner_disunity_runtime::{
    CompatibilityHost, HostError, ManagedHostConfig, NativeImageLease,
};

mod abi;

static FILE: OnceLock<File> = OnceLock::new();

fn config() -> ManagedHostConfig {
    let managed = std::path::PathBuf::from(std::env::var_os("DISUNITY_TEST_MANAGED_ROOT").unwrap());
    let assembly = managed.join("artifacts/bin/CompatibilityBridge/debug/CompatibilityBridge.dll");
    ManagedHostConfig {
        ffi_library: std::env::var_os("DISUNITY_TEST_NATIVE_LIBRARY")
            .unwrap()
            .into(),
        runtime_config: assembly.with_extension("runtimeconfig.json"),
        assembly,
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    }
}

fn sealed() -> File {
    let bytes = std::fs::read(config().ffi_library).unwrap();
    let fd = memfd_create(
        "lifetime-selected-native",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )
    .unwrap();
    let mut file = File::from(fd);
    file.write_all(&bytes).unwrap();
    fcntl_add_seals(
        &file,
        SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
    )
    .unwrap();
    File::open(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap()
}

fn lease(file: File) -> NativeImageLease {
    let bytes = std::fs::read(config().ffi_library).unwrap();
    NativeImageLease::from_file(file, &format!("{:x}", Sha256::digest(&bytes))).unwrap()
}

fn boot(mode: &str, config: &ManagedHostConfig) -> Result<CompatibilityHost, HostError> {
    if mode.starts_with("sealed-") {
        let file = FILE.get_or_init(sealed);
        let mut selected = config.clone();
        selected.ffi_library = "/must-not-load-this-unverified-image".into();
        CompatibilityHost::boot_sealed_native(&selected, lease(file.try_clone().unwrap()))
    } else {
        CompatibilityHost::boot(config)
    }
}

pub fn exercise(mode: &str) {
    let mut config = config();
    if mode.ends_with("early") {
        let path = if mode.starts_with("sealed-") {
            format!("/proc/self/fd/{}", FILE.get_or_init(sealed).as_raw_fd()).into()
        } else {
            config.ffi_library.clone()
        };
        let library = abi::initialize_thread(&path);
        config.bridge_type = "Missing.Bridge, Missing.Assembly".into();
        assert!(matches!(boot(mode, &config), Err(HostError::Hostfxr(_))));
        drop(library);
        return;
    }
    let mut host = boot(mode, &config).unwrap();
    host.create_world().unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 1);
    if mode.ends_with("unwind") {
        panic!("intentional unwind with a live native world");
    }
    drop(host);
}

pub fn verify_drained(mode: &str) {
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
    let selected = config();
    let metadata = if mode.starts_with("sealed-") {
        FILE.get().unwrap().metadata().unwrap()
    } else {
        std::fs::metadata(&selected.ffi_library).unwrap()
    };
    let inode = metadata.ino().to_string();
    assert!(
        maps.lines()
            .any(|line| line.split_whitespace().nth(4) == Some(&inode))
    );
    eprintln!(
        "IMAGE {mode}: inode={inode} sha256={:x}",
        Sha256::digest(std::fs::read(&selected.ffi_library).unwrap())
    );
    let host = boot(mode, &config()).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert_eq!(roots["data"]["scripts"], 0);
    assert_eq!(roots["data"]["bindings"], 0);
    assert_eq!(roots["data"]["coroutines"], 0);
    assert_eq!(roots["data"]["loaded"], false);
    drop(host);
    if mode.starts_with("sealed-") {
        let error = CompatibilityHost::boot_sealed_native(&config(), lease(sealed()))
            .err()
            .expect("a different sealed inode must be rejected");
        assert!(error.to_string().contains("different process image"));
    }
}
