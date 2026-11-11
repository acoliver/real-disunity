//! Serialized compute shader objects for tests: a player's compute shader with one SPIR-V
//! platform that lists its kernels, written by the layout writer of the unity-serialized tests.

#![allow(dead_code)]

#[path = "../../../unity-serialized/tests/support/compute_bytes.rs"]
mod layout;

use layout::{Binding, Kernel, Platform, SPIRV, Variant, compute_shader_bytes};

/// A compute shader named `name` with `kernels`, each with the thread group size it states.
pub fn compute_shader(name: &str, kernels: &[(&str, [u32; 3])]) -> Vec<u8> {
    let kernels = kernels
        .iter()
        .map(|&(kernel_name, size)| {
            let mut variant = Variant::new(&[0x03, 0x02, 0x23, 0x07], size);
            variant.constant_buffers = vec![Binding::constant_buffer("$Globals", 1, 0)];
            variant.textures = vec![Binding::texture("_Source", 0, None, 2)];
            variant.writable = vec![Binding::writable("_Output", 1)];
            Kernel::single(kernel_name, variant)
        })
        .collect();
    compute_shader_bytes(name, &[Platform::new(SPIRV, 0, kernels)], false)
}
