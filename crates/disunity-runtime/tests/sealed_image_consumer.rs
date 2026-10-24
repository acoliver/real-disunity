#![cfg(target_os = "linux")]

#[path = "sealed_image_helpers/mechanism.rs"]
mod mechanism;
#[path = "sealed_image_helpers/shared.rs"]
mod shared;

use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;

use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
use sha2::{Digest, Sha256};
use worldrunner_disunity_runtime::NativeImageLease;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sealed(bytes: &[u8]) -> File {
    let fd = memfd_create(
        "sealed-native-attachment",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )
    .unwrap();
    let mut file = File::from(fd);
    file.write_all(bytes).unwrap();
    fcntl_add_seals(
        &file,
        SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
    )
    .unwrap();
    File::open(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap()
}

#[test]
fn verified_descriptor_is_independent_of_source_owner_and_offset() {
    let bytes = b"opaque host selected image bytes";
    let file = sealed(bytes);
    let mut duplicate = file.try_clone().unwrap();
    std::io::Read::read_to_end(&mut duplicate, &mut Vec::new()).unwrap();
    let lease = NativeImageLease::from_file(file, &digest(bytes)).unwrap();
    drop(duplicate);
    assert_eq!(lease.sha256(), digest(bytes));
    assert_eq!(lease.len(), bytes.len() as u64);
    assert!(!lease.is_empty());
}
