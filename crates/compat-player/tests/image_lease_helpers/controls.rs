use super::*;
use std::{
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{FileExt, symlink},
    sync::{Arc, Barrier},
};

#[test]
fn original_mutation_replacement_and_unlink_leave_sealed_bytes_unchanged() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    let path = root.path().join("image");
    fs::write(&path, b"verified").unwrap();
    let lease =
        SealedImage::capture(root.path(), Path::new("image"), &hash(b"verified"), 64).unwrap();
    for bytes in [&b"modified"[..], b"longer replacement bytes", b"x", b""] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(fs::read(lease.load_path().as_path()).unwrap(), b"verified");
    }
    fs::rename(&path, root.path().join("moved")).unwrap();
    fs::write(&path, b"replacement").unwrap();
    fs::remove_file(&path).unwrap();
    fs::copy(root.path().join("moved"), &path).unwrap();
    fs::remove_file(root.path().join("moved")).unwrap();
    assert_eq!(
        hash(&fs::read(lease.load_path().as_path()).unwrap()),
        lease.sha256()
    );
    let writer = OpenOptions::new()
        .write(true)
        .open(lease.load_path().as_path())
        .unwrap();
    assert_eq!(
        writer.write_at(b"x", 0).unwrap_err().raw_os_error(),
        Some(1)
    );
    assert_eq!(
        writer.write_at(b"x", 64).unwrap_err().raw_os_error(),
        Some(1)
    );
}

#[test]
fn readonly_inode_pin_still_observes_in_place_rewrite() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    let path = root.path().join("image");
    fs::write(&path, b"verified").unwrap();
    let mut pinned = File::open(&path).unwrap();
    let expected = hash(&fs::read(&path).unwrap());
    fs::write(&path, b"modified").unwrap();
    pinned.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    pinned.read_to_end(&mut bytes).unwrap();
    assert_ne!(hash(&bytes), expected);
    assert_eq!(bytes, b"modified");
}

#[test]
fn path_resolution_rejects_symlinks_and_non_normal_paths() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    fs::create_dir(root.path().join("sub")).unwrap();
    fs::write(root.path().join("sub/image"), b"valid").unwrap();
    symlink("sub/image", root.path().join("link")).unwrap();
    symlink("sub", root.path().join("intermediate")).unwrap();
    symlink("absent", root.path().join("dangling")).unwrap();
    symlink("/proc/self/fd/0", root.path().join("magic")).unwrap();
    for path in [
        "",
        ".",
        "..",
        "../escape",
        "/etc/passwd",
        "sub/../sub/image",
        "sub//image",
        "sub/./image",
        "sub/image/",
        "link",
        "intermediate/image",
        "dangling",
        "magic",
        "sub",
    ] {
        assert!(
            SealedImage::capture(root.path(), Path::new(path), &hash(b"valid"), 64).is_err(),
            "{path}"
        );
    }
    let alias = root.path().join("alias");
    symlink(root.path().join("sub"), &alias).unwrap();
    assert!(SealedImage::capture(&alias, Path::new("image"), &hash(b"valid"), 64).is_ok());
}

#[test]
fn declared_limits_file_types_and_identity_fail_closed() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    let path = root.path().join("image");
    fs::write(&path, b"valid").unwrap();
    for maximum in [0, 4, image_lease::MAX_IMAGE_BYTES + 1, u64::MAX] {
        assert!(
            SealedImage::capture(root.path(), Path::new("image"), &hash(b"valid"), maximum)
                .is_err()
        );
    }
    for digest in [
        String::new(),
        "f".repeat(63),
        "f".repeat(65),
        "F".repeat(64),
        "g".repeat(64),
        "0".repeat(64),
    ] {
        assert!(SealedImage::capture(root.path(), Path::new("image"), &digest, 64).is_err());
    }
    fs::write(&path, b"").unwrap();
    assert!(SealedImage::capture(root.path(), Path::new("image"), &hash(b""), 64).is_err());
    let file = OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(image_lease::MAX_IMAGE_BYTES + 1).unwrap();
    assert!(
        SealedImage::capture(
            root.path(),
            Path::new("image"),
            &hash(b""),
            image_lease::MAX_IMAGE_BYTES
        )
        .is_err()
    );
    rustix::fs::mknodat(
        rustix::fs::CWD,
        root.path().join("fifo"),
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR,
        0,
    )
    .unwrap();
    assert!(SealedImage::capture(root.path(), Path::new("fifo"), &hash(b""), 64).is_err());
}

#[test]
fn descriptor_lifetime_and_repeated_failure_release_resources() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    fs::write(root.path().join("image"), b"verified").unwrap();
    let count = || fs::read_dir("/proc/self/fd").unwrap().count();
    let before = count();
    for _ in 0..64 {
        assert!(
            SealedImage::capture(root.path(), Path::new("image"), &"0".repeat(64), 64).is_err()
        );
        let lease =
            SealedImage::capture(root.path(), Path::new("image"), &hash(b"verified"), 64).unwrap();
        let file = File::open(lease.load_path().as_path()).unwrap();
        assert!(
            rustix::io::fcntl_getfd(&file)
                .unwrap()
                .contains(rustix::io::FdFlags::CLOEXEC)
        );
        drop(file);
        let path = lease.load_path().as_path().to_path_buf();
        drop(lease);
        assert!(!path.exists());
    }
    assert_eq!(count(), before);
}

#[test]
fn concurrent_source_rewrite_returns_expected_bytes_or_error() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    let path = root.path().join("image");
    let bytes = vec![0x31; 4 * 1024 * 1024];
    let expected = hash(&bytes);
    for _ in 0..8 {
        fs::write(&path, &bytes).unwrap();
        let start = Arc::new(Barrier::new(2));
        let writer_start = Arc::clone(&start);
        let writer_path = path.clone();
        let writer = std::thread::spawn(move || {
            let file = OpenOptions::new().write(true).open(writer_path).unwrap();
            writer_start.wait();
            file.write_all_at(&vec![0x32; 64 * 1024], 1024 * 1024)
                .unwrap();
        });
        start.wait();
        let captured = SealedImage::capture(
            root.path(),
            Path::new("image"),
            &expected,
            bytes.len() as u64,
        );
        writer.join().unwrap();
        if let Ok(lease) = captured {
            assert_eq!(
                hash(&fs::read(lease.load_path().as_path()).unwrap()),
                expected
            );
        }
    }
}
