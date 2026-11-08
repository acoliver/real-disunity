//! The census of a player's compute kernels on a device: every kernel variant of the SPIR-V
//! platform is built as a compute program and the ones that are not are counted by cause.

use worldrunner_render_program::unity::synthetic::compute_spirv;

use super::super::census::compute_census;
use super::gpu::gpu;
use super::kernels::compute_bytes::{Binding, Kernel, Variant};
use super::kernels::{copy_object, shader_bytes, text_object, write_compute};

/// A kernel that writes `images` storage images of one binding each.
fn painter(images: u32) -> Vec<u8> {
    let declarations: String = (0..images)
        .map(|i| {
            format!("@group(0) @binding({i}) var image{i}: texture_storage_2d<r32float, write>;\n")
        })
        .collect();
    let stores: String = (0..images)
        .map(|i| format!("textureStore(image{i}, vec2<i32>(id.xy), vec4<f32>(1.0));\n"))
        .collect();
    let wgsl = format!(
        "{declarations}
         @compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
             {stores}
         }}"
    );
    let code: Vec<u8> = compute_spirv(&wgsl, "main")
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    let mut variant = Variant::new(&code, [2, 2, 1]);
    variant.writable = (0..images)
        .map(|i| Binding::image(&format!("_Image{i}"), i, 2))
        .collect();
    shader_bytes("Paint", vec![Kernel::single("Paint", variant)])
}

#[test]
fn every_variant_is_built_on_the_device_and_the_ones_it_cannot_hold_are_counted_by_cause() {
    let directory = tempfile::tempdir().unwrap();
    // The copy kernel, a kernel with four storage images, one with five (the test device allows
    // four in a stage) and a shader with no SPIR-V platform, which has no variants to count.
    write_compute(
        directory.path(),
        "resources.assets",
        vec![
            (1, copy_object()),
            (2, painter(4)),
            (3, painter(5)),
            (4, text_object()),
        ],
    );
    let runtime = gpu().runtime;

    let census = compute_census(directory.path(), &runtime).unwrap();

    assert_eq!(census.shaders, 4);
    assert_eq!((census.variants, census.programs), (3, 2));
    assert_eq!(
        census.failed.into_iter().collect::<Vec<_>>(),
        [("StorageLimit", 1)]
    );
}
