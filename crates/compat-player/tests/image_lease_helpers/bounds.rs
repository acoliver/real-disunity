use super::*;
use std::sync::{Arc, Barrier};

#[test]
fn exact_minimum_and_hard_maximum_are_captured_without_truncation() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    let path = root.path().join("image");
    for length in [1, usize::try_from(image_lease::MAX_IMAGE_BYTES).unwrap()] {
        let bytes = vec![0x57; length];
        fs::write(&path, &bytes).unwrap();
        let lease = SealedImage::capture(
            root.path(),
            Path::new("image"),
            &hash(&bytes),
            length as u64,
        )
        .unwrap();
        assert_eq!(lease.len(), length as u64);
        assert_eq!(fs::read(lease.load_path().as_path()).unwrap(), bytes);
    }
}

#[test]
fn concurrent_truncation_and_growth_return_host_expected_bytes_or_error() {
    let _serial = SERIAL.lock().unwrap();
    let root = directory();
    let path = root.path().join("image");
    let bytes = vec![0x39; 1024 * 1024];
    let expected = hash(&bytes);
    for length in [0, 1, 1024 * 1024 + 1, 2 * 1024 * 1024] {
        for _ in 0..4 {
            fs::write(&path, &bytes).unwrap();
            let file = OpenOptions::new().write(true).open(&path).unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let start = Arc::clone(&barrier);
            let writer = std::thread::spawn(move || {
                start.wait();
                file.set_len(length).unwrap();
            });
            barrier.wait();
            let result =
                SealedImage::capture(root.path(), Path::new("image"), &expected, 2 * 1024 * 1024);
            writer.join().unwrap();
            if let Ok(lease) = result {
                assert_eq!(fs::read(lease.load_path().as_path()).unwrap(), bytes);
            }
        }
    }
}
