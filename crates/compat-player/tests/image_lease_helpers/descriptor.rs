use super::*;
use std::os::fd::AsRawFd;
use std::process::Command;

#[test]
fn descriptor_is_cloexec_and_not_transferred_to_exec_child() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    fs::write(root.path().join("image"), b"verified").unwrap();
    let lease =
        SealedImage::capture(root.path(), Path::new("image"), &hash(b"verified"), 8).unwrap();
    let borrowed = lease.load_path();
    let descriptor = borrowed.as_path().file_name().unwrap().to_str().unwrap();
    let info = fs::read_to_string(format!("/proc/self/fdinfo/{descriptor}")).unwrap();
    let flags = info
        .lines()
        .find_map(|line| line.strip_prefix("flags:\t"))
        .unwrap();
    assert_ne!(u64::from_str_radix(flags, 8).unwrap() & 0o2_000_000, 0);
    let script = "import os,sys; p='/proc/self/fd/'+sys.argv[1]; assert not os.path.exists(p), p";
    let output = Command::new("python3")
        .args(["-c", script, descriptor])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(borrowed.as_path()).unwrap(), b"verified");
}

#[test]
fn ordinary_readonly_pin_observes_truncate_grow_rename_and_unlink() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    let path = root.path().join("image");
    fs::write(&path, b"verified").unwrap();
    let pinned = File::open(&path).unwrap();
    let pinned_path = format!("/proc/self/fd/{}", pinned.as_raw_fd());
    for bytes in [&b"x"[..], b"longer rewritten image", b""] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(fs::read(&pinned_path).unwrap(), bytes);
    }
    fs::write(&path, b"old inode").unwrap();
    fs::rename(&path, root.path().join("moved")).unwrap();
    fs::write(&path, b"new inode").unwrap();
    fs::remove_file(root.path().join("moved")).unwrap();
    assert_eq!(fs::read(pinned_path).unwrap(), b"old inode");
    assert_eq!(fs::read(path).unwrap(), b"new inode");
}
