//! A serialized `Font` object for tests.

use worldrunner_unity_serialized::{object::PPtr, write::ObjectWriter};

/// A font named `name` whose font file is `file`.
pub fn font(name: &str, file: &[u8]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    let none = PPtr {
        file_id: 0,
        path_id: 0,
    };
    w.string(name)
        .unwrap()
        .f32(1.)
        .pptr(none)
        .f32(16.)
        .pptr(none);
    w.i32(0).f32(0.).i32(0).i32(0).i32(-2).i32(0).i32(0);
    w.f32(1.).byte_array(file).unwrap();
    w.f32(12.).f32(-3.).u32(0).i32(0).pptrs(&[]).unwrap();
    w.i32(0).bytes(&[0, 0]);
    w.finish()
}
