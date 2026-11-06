//! Serialized compute shader objects for tests: the layout of a player's compute shader with one
//! platform that lists its kernels.

use worldrunner_unity_serialized::write::ObjectWriter;

fn resources(w: &mut ObjectWriter, names: &[&str]) {
    w.u32(u32::try_from(names.len()).unwrap());
    for name in names {
        w.string(name).unwrap();
        for field in [0, 1, u32::MAX, u32::MAX, 0] {
            w.u32(field);
        }
    }
}

fn kernel(w: &mut ObjectWriter, name: &str, size: [u32; 3]) {
    w.string(name).unwrap();
    w.u32(1).u32(1).u32(0);
    resources(w, &["$Globals"]);
    resources(w, &["_Source"]);
    w.u32(0);
    resources(w, &[]);
    resources(w, &["_Output"]);
    w.byte_array(&[0x44, 0x58, 0x42, 0x43]).unwrap();
    w.u32(3).u32(size[0]).u32(size[1]).u32(size[2]);
    w.u32(50241).u32(0).u32(1).u32(0);
    for _ in 0..4 {
        w.u32(0);
    }
}

/// A compute shader named `name` with `kernels`, each with the thread group size it states.
pub fn compute_shader(name: &str, kernels: &[(&str, [u32; 3])]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap();
    w.u32(1).u32(2).u32(0);
    w.u32(u32::try_from(kernels.len()).unwrap());
    for (kernel_name, size) in kernels {
        kernel(&mut w, kernel_name, *size);
    }
    resources(&mut w, &["$Globals"]);
    w.finish()
}
