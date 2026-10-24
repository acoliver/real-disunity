use super::*;
use rustix::io::{FdFlags, fcntl_getfd};
use std::fs;
use std::os::unix::fs::FileExt;
use worldrunner_disunity_runtime::{CompatibilityHost, HostError, ManagedHostConfig};

#[test]
fn ordinary_readonly_file_and_nonregular_file_are_rejected() {
    let path = std::env::current_exe().unwrap();
    assert!(NativeImageLease::from_file(File::open(path).unwrap(), &"0".repeat(64)).is_err());
    assert!(NativeImageLease::from_file(File::open("/dev/null").unwrap(), &digest(b"x")).is_err());
}

#[test]
fn digest_syntax_and_sealed_digest_mismatch_are_rejected() {
    for expected in ["", "abc", &"A".repeat(64), &"g".repeat(64), &"0".repeat(64)] {
        assert!(NativeImageLease::from_file(sealed(b"x"), expected).is_err());
    }
}

#[test]
fn empty_and_oversize_sparse_descriptors_are_rejected() {
    assert!(NativeImageLease::from_file(sealed(b""), &digest(b"")).is_err());
    let fd = memfd_create("oversize", MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING).unwrap();
    let file = File::from(fd);
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    fcntl_add_seals(
        &file,
        SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
    )
    .unwrap();
    let readonly = File::open(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap();
    assert!(NativeImageLease::from_file(readonly, &"0".repeat(64)).is_err());
}

#[test]
fn missing_effective_seals_are_rejected() {
    for flags in [
        SealFlags::empty(),
        SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
    ] {
        let fd = memfd_create("incomplete", MemfdFlags::ALLOW_SEALING).unwrap();
        let mut file = File::from(fd);
        file.write_all(b"x").unwrap();
        fcntl_add_seals(&file, flags).unwrap();
        let readonly = File::open(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap();
        assert!(NativeImageLease::from_file(readonly, &digest(b"x")).is_err());
    }
}

#[test]
fn retained_descriptor_is_cloexec_and_immutable() {
    let file = sealed(b"sealed");
    let peer = file.try_clone().unwrap();
    let lease = NativeImageLease::from_file(file, &digest(b"sealed")).unwrap();
    assert!(fcntl_getfd(&peer).unwrap().contains(FdFlags::CLOEXEC));
    let writer = fs::OpenOptions::new()
        .write(true)
        .open(format!("/proc/self/fd/{}", peer.as_raw_fd()))
        .unwrap();
    assert_eq!(
        writer.write_at(b"changed", 0).unwrap_err().raw_os_error(),
        Some(1)
    );
    assert!(writer.set_len(1).is_err());
    assert!(writer.set_len(100).is_err());
    drop(peer);
    assert_eq!(lease.sha256(), digest(b"sealed"));
}

#[test]
fn invalid_managed_config_drops_unconfigured_lease_without_claiming_host() {
    let config = ManagedHostConfig {
        ffi_library: "/deliberately/ignored/native/path".into(),
        runtime_config: "/missing/runtimeconfig".into(),
        assembly: "/missing/assembly".into(),
        bridge_type: "Explicit.Bridge, Explicit".into(),
    };
    for _ in 0..8 {
        let result = CompatibilityHost::boot_sealed_native(
            &config,
            NativeImageLease::from_file(sealed(b"not executable"), &digest(b"not executable"))
                .unwrap(),
        );
        assert!(matches!(result, Err(HostError::ManagedArtifactsMissing(_))));
    }
}

#[test]
fn repeated_success_and_failure_do_not_retain_per_lease_descriptors() {
    let count = || fs::read_dir("/proc/self/fd").unwrap().count();
    let before = count();
    for _ in 0..64 {
        let image = NativeImageLease::from_file(sealed(b"x"), &digest(b"x")).unwrap();
        drop(image);
        assert!(NativeImageLease::from_file(sealed(b"x"), &"0".repeat(64)).is_err());
    }
    assert_eq!(count(), before);
}

#[test]
fn native_load_failure_releases_claim_and_unconfigured_descriptor() {
    let executable = std::env::current_exe().unwrap();
    let config = ManagedHostConfig {
        ffi_library: "/ignored/native".into(),
        runtime_config: executable.clone(),
        assembly: executable,
        bridge_type: "Explicit.Bridge, Explicit".into(),
    };
    let count = || fs::read_dir("/proc/self/fd").unwrap().count();
    let before = count();
    for _ in 0..8 {
        let lease =
            NativeImageLease::from_file(sealed(b"not an ELF image"), &digest(b"not an ELF image"))
                .unwrap();
        assert!(matches!(
            CompatibilityHost::boot_sealed_native(&config, lease),
            Err(HostError::FfiNotFound(_))
        ));
    }
    assert_eq!(count(), before);
}
