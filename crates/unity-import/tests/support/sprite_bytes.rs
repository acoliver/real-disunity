//! Serialized texture and sprite objects for tests.

use worldrunner_unity_serialized::{object::PPtr, write::ObjectWriter};

/// Where a texture keeps its pixels.
pub enum Pixels {
    Inline(&'static [u8]),
    /// A byte range of the named `.resS` file.
    Streamed(&'static str, u64, u32),
}

/// A 2x2 RGBA32 texture of one mip level with the sampler settings bilinear, 4x aniso, a bias
/// of -0.5 and wrap modes repeat, clamp and mirror, in the sRGB color space (value 1).
pub fn texture(name: &str, images: i32, dimension: i32, pixels: &Pixels) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().u8(0).align();
    w.i32(2).i32(2).u32(16).i32(0).i32(4).i32(1);
    w.u8(1).u8(0).u8(0).align();
    w.string("").unwrap().u8(0).align().i32(0).align();
    w.i32(images).i32(dimension);
    w.i32(1).i32(4).f32(-0.5).i32(0).i32(1).i32(2);
    w.i32(6).i32(1).byte_array(&[]).unwrap();
    match pixels {
        Pixels::Inline(bytes) => {
            w.byte_array(bytes)
                .unwrap()
                .u64(0)
                .u32(0)
                .string("")
                .unwrap();
        }
        Pixels::Streamed(path, offset, size) => {
            w.byte_array(&[])
                .unwrap()
                .u64(*offset)
                .u32(*size)
                .string(path)
                .unwrap();
        }
    }
    w.finish()
}

/// A sprite named `name` of a 16x8 rectangle at (2, 4) with a half pivot, 50 pixels per unit and
/// a border, drawn from the texture object `texture` of its own file (none when 0), with one bone
/// when `bones` is set.
pub fn sprite(name: &str, texture: i64, bones: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap();
    w.floats(&[2.0, 4.0, 16.0, 8.0]).floats(&[0.0, 0.0]);
    w.floats(&[1.0, 2.0, 3.0, 4.0])
        .f32(50.0)
        .floats(&[0.5, 0.25]);
    w.u32(1).u8(0).align();
    w.u32(1).u32(2).u32(3).u32(4).i64(-7);
    w.i32(0).pptr(PPtr {
        file_id: 0,
        path_id: 0,
    });
    w.pptr(PPtr {
        file_id: 0,
        path_id: texture,
    })
    .pptr(PPtr {
        file_id: 0,
        path_id: 0,
    });
    w.i32(0).i32(0).byte_array(&[]).unwrap();
    w.u32(0).i32(0).byte_array(&[]).unwrap().align();
    w.i32(0).align();
    w.floats(&[3.0, 5.0, 14.0, 6.0])
        .floats(&[1.0, 0.5])
        .floats(&[-1.0, -1.0]);
    w.u32(1 | (3 << 2) | (1 << 6))
        .floats(&[50.0, 8.0, 50.0, 2.0])
        .f32(1.0);
    w.i32(0).align();
    if bones {
        w.i32(1).string("root").unwrap().string("guid").unwrap();
        w.floats(&[0.0; 3])
            .floats(&[0.0, 0.0, 0.0, 1.0])
            .f32(1.0)
            .i32(-1)
            .u32(0);
    } else {
        w.i32(0);
    }
    w.align().pptrs(&[]).unwrap();
    w.finish()
}
