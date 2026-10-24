#![cfg(target_os = "linux")]

use compat_player::image_lease;

use image_lease::SealedImage;
use rustix::fs::{SealFlags, fcntl_add_seals, fcntl_get_seals};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[path = "image_lease_helpers/bounds.rs"]
mod bounds;
#[path = "image_lease_helpers/controls.rs"]
mod controls;
#[path = "image_lease_helpers/descriptor.rs"]
mod descriptor;

#[path = "image_lease_helpers/consumer.rs"]
mod consumer;
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn directory() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = if root.ends_with("compat-player") {
        root.join("../../.work/resumed-image-leases/test-tmp")
    } else {
        root.join("test-tmp")
    };
    fs::create_dir_all(&work).unwrap();
    tempfile::tempdir_in(work).unwrap()
}

#[test]
fn sealed_bytes_survive_source_overwrite_and_kernel_rejects_writes() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    fs::write(root.path().join("image"), b"original").unwrap();
    let lease =
        SealedImage::capture(root.path(), Path::new("image"), &hash(b"original"), 8).unwrap();
    fs::write(root.path().join("image"), b"modified").unwrap();
    assert_eq!(fs::read(lease.load_path().as_path()).unwrap(), b"original");
    assert_eq!(lease.sha256(), hash(b"original"));
    assert_eq!(lease.len(), 8);
    assert!(!lease.is_empty());
    assert_eq!(lease.maximum(), 8);
    let mut writer = OpenOptions::new()
        .write(true)
        .open(lease.load_path().as_path())
        .unwrap();
    assert_eq!(writer.write(b"x").unwrap_err().raw_os_error(), Some(1));
    assert_eq!(writer.set_len(0).unwrap_err().raw_os_error(), Some(1));
    assert_eq!(writer.set_len(9).unwrap_err().raw_os_error(), Some(1));
    let file = File::open(lease.load_path().as_path()).unwrap();
    let required = SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL;
    assert!(fcntl_get_seals(&file).unwrap().contains(required));
    assert_eq!(
        fcntl_add_seals(&file, SealFlags::empty()).unwrap_err(),
        rustix::io::Errno::PERM
    );
}
